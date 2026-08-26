//! Hashing phase: concurrently download each unhashed asset and store its
//! Nix-base32 SHA-256 in SQLite. Every update persists immediately, so runs
//! are resumable without explicit checkpoints.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::{bail, Result};
use diesel::prelude::*;
use futures::stream::{self, StreamExt};
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::{
    api::GnomeClient,
    nix_hash::sha256_nix_base32,
    schema::assets::{self, dsl},
};

const PROGRESS_EVERY: usize = 50;

pub async fn fetch_hash(
    client: &GnomeClient,
    conn: Arc<Mutex<SqliteConnection>>,
    batch_size: usize,
    limit: usize,
) -> Result<usize> {
    let (targets, missing_total): (Vec<(i32, String)>, usize) = {
        let mut db = conn.lock().await;
        let all: Vec<(i32, String)> = dsl::assets
            .filter(dsl::hash.is_null().or(dsl::hash.eq("")))
            .order_by(dsl::version_tag)
            .select((dsl::version_tag, dsl::uuid))
            .load(&mut *db)?;
        let total = all.len();
        let targets = if limit == 0 {
            all
        } else {
            all.into_iter().take(limit).collect()
        };
        (targets, total)
    };
    info!(
        total_missing = missing_total,
        selected = targets.len(),
        batch_size,
        "hashing assets"
    );

    let completed = Arc::new(AtomicUsize::new(0));

    stream::iter(targets)
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
