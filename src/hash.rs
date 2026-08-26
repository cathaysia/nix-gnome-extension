//! Hashing phase: stream unhashed assets out of SQLite through an
//! async-stream generator into a bounded concurrency pool (same shape as
//! nix4vscode). Every hash persists immediately, so runs are resumable.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::{bail, Result};
use async_stream::stream;
use diesel::prelude::*;
use futures::stream::{Stream, StreamExt};
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::{
    api::GnomeClient,
    nix_hash::sha256_nix_base32,
    schema::assets::{self, dsl},
};

const PROGRESS_EVERY: usize = 50;

/// Rows fetched per DB round-trip while streaming.
const CHUNK: usize = 500;

/// Stream `(version_tag, uuid)` pairs of unhashed assets.
///
/// Paged by key range (`version_tag > last`) rather than OFFSET so the window
/// stays stable while the pool removes rows from the filtered set underneath.
fn missing_targets(
    conn: Arc<Mutex<SqliteConnection>>,
    limit: usize,
) -> impl Stream<Item = (i32, String)> {
    stream! {
        let mut last_tag: i32 = 0;
        let mut yielded: usize = 0;
        loop {
            let chunk: Vec<(i32, String)> = {
                let mut db = conn.lock().await;
                dsl::assets
                    .filter(dsl::version_tag.gt(last_tag))
                    .filter(dsl::hash.is_null().or(dsl::hash.eq("")))
                    .order_by(dsl::version_tag)
                    .limit(CHUNK as i64)
                    .select((dsl::version_tag, dsl::uuid))
                    .load(&mut *db)
                    .unwrap_or_default()
            };
            if chunk.is_empty() {
                break;
            }
            last_tag = chunk[chunk.len() - 1].0;
            for target in chunk {
                if limit != 0 && yielded >= limit {
                    return;
                }
                yielded += 1;
                yield target;
            }
        }
    }
}

pub async fn fetch_hash(
    client: &GnomeClient,
    conn: Arc<Mutex<SqliteConnection>>,
    batch_size: usize,
    limit: usize,
) -> Result<usize> {
    let total_missing: i64 = {
        let mut db = conn.lock().await;
        dsl::assets
            .filter(dsl::hash.is_null().or(dsl::hash.eq("")))
            .select(diesel::dsl::count_star())
            .get_result(&mut *db)?
    };
    info!(total_missing, batch_size, limit, "hashing assets");

    let completed = Arc::new(AtomicUsize::new(0));

    let targets = missing_targets(Arc::clone(&conn), limit);
    tokio::pin!(targets);

    targets
        .map(|(tag, uuid)| {
            let conn = Arc::clone(&conn);
            let completed = Arc::clone(&completed);
            async move {
                match hash_asset(client, &uuid, tag).await {
                    Ok(digest) => {
                        let mut db = conn.lock().await;
                        if let Err(err) = diesel::update(assets::table.find(tag))
                            .set(dsl::hash.eq(Some(digest)))
                            .execute(&mut *db)
                        {
                            error!(tag, ?err, "failed to persist hash");
                            return;
                        }
                        drop(db);
                        let n = completed.fetch_add(1, Ordering::SeqCst) + 1;
                        if n.is_multiple_of(PROGRESS_EVERY) {
                            info!(done = n, "progress");
                        }
                    }
                    Err(err) => {
                        error!(tag, %uuid, ?err, "hash failed; will retry on next run");
                    }
                }
            }
        })
        .buffer_unordered(batch_size)
        .for_each(|_| async {})
        .await;

    Ok(completed.load(Ordering::SeqCst))
}

pub fn missing_count(conn: &mut SqliteConnection) -> Result<usize> {
    let count: i64 = dsl::assets
        .filter(dsl::hash.is_null().or(dsl::hash.eq("")))
        .select(diesel::dsl::count_star())
        .get_result(conn)?;
    Ok(count as usize)
}

async fn hash_asset(client: &GnomeClient, uuid: &str, tag: i32) -> Result<String> {
    let url = GnomeClient::download_url(uuid, tag as u64);
    let data = client.get_bytes(&url).await?;
    if data.is_empty() {
        bail!("empty body for {uuid} tag {tag}");
    }
    Ok(sha256_nix_base32(&data))
}
