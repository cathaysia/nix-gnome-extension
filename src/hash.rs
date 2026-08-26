//! Hashing phase: concurrently download each asset and compute its
//! Nix-base32 SHA-256. Progress is checkpointed into the state file so runs
//! are resumable.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::{bail, Result};
use futures::stream::{self, StreamExt};
use tokio::sync::Mutex;
use tracing::{debug, error, info};

use crate::{api::GnomeClient, nix_hash::sha256_nix_base32, state::State};

const SAVE_EVERY: usize = 50;

pub async fn fetch_hash(
    client: &GnomeClient,
    state: Arc<Mutex<State>>,
    state_path: PathBuf,
    batch_size: usize,
    limit: usize,
) -> Result<usize> {
    let (targets, missing_total): (Vec<(u64, String)>, usize) = {
        let st = state.lock().await;
        let missing_total = st.missing_hashes();
        let targets = st
            .rows
            .iter()
            .filter(|(_, row)| row.hash.as_deref().is_none_or(str::is_empty))
            .take(if limit == 0 { usize::MAX } else { limit })
            .map(|(tag, row)| (*tag, row.uuid.clone()))
            .collect();
        (targets, missing_total)
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
            let state = Arc::clone(&state);
            let state_path = state_path.clone();
            let completed = Arc::clone(&completed);
            async move {
                match hash_asset(client, &uuid, tag).await {
                    Ok(digest) => {
                        {
                            let mut st = state.lock().await;
                            if let Some(row) = st.rows.get_mut(&tag) {
                                row.hash = Some(digest);
                            }
                        }
                        let n = completed.fetch_add(1, Ordering::SeqCst) + 1;
                        if n.is_multiple_of(SAVE_EVERY) {
                            let st = state.lock().await;
                            if let Err(err) = st.save(&state_path) {
                                error!(?err, "checkpoint save failed");
                            }
                            debug!(done = n, "checkpoint saved");
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

    // Final save with whatever progress was made.
    {
        let st = state.lock().await;
        st.save(&state_path)?;
    }

    Ok(completed.load(Ordering::SeqCst))
}

async fn hash_asset(client: &GnomeClient, uuid: &str, tag: u64) -> Result<String> {
    let url = GnomeClient::download_url(uuid, tag);
    let data = client.get_bytes(&url).await?;
    if data.is_empty() {
        bail!("empty body for {uuid} tag {tag}");
    }
    Ok(sha256_nix_base32(&data))
}
