mod api;
mod export;
mod fetch;
mod hash;
mod nix_hash;
mod state;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Result};
use clap::Parser;
use tokio::sync::Mutex;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
struct Args {
    /// Fetch extension list from extensions.gnome.org into the state file.
    #[arg(long)]
    fetch: bool,

    /// Compute hashes for assets that do not have one yet.
    #[arg(long)]
    hash: bool,

    /// Directory to write the sharded JSON data files to.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Resumable state file path.
    #[arg(long, default_value = "state.json")]
    state: PathBuf,

    /// Concurrency of the hashing download pool.
    #[arg(long, default_value_t = 8)]
    batch_size: usize,

    /// Abort hashing after N seconds, keeping partial progress (0 = unlimited).
    #[arg(long, default_value_t = 0)]
    max_run_time: u64,

    /// Fetch at most N query pages this run (0 = all).
    #[arg(long, default_value_t = 0)]
    max_pages: u64,

    /// Hash at most N assets this run (0 = all missing).
    #[arg(long, default_value_t = 0)]
    limit: usize,

    /// Enumeration order passed to extension-query (?sort=).
    #[arg(long, default_value = "name")]
    sort: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    let args = Args::parse();

    if !args.fetch && !args.hash && args.output.is_none() {
        bail!("nothing to do: pass --fetch, --hash and/or --output <dir>");
    }

    let client = api::GnomeClient::new()?;
    let state = Arc::new(Mutex::new(state::State::load(&args.state)?));
    info!(state = %args.state.display(), rows = state.lock().await.rows.len(), "state loaded");

    if args.fetch {
        let (extensions, added) = fetch::fetch_extensions(
            &client,
            &mut *state.lock().await,
            &args.sort,
            args.max_pages,
        )
        .await?;
        let locked = state.lock().await;
        info!(extensions, added, assets = locked.rows.len(), "fetch done");
        locked.save(&args.state)?;
    }

    if args.hash {
        let run = hash::fetch_hash(
            &client,
            Arc::clone(&state),
            args.state.clone(),
            args.batch_size,
            args.limit,
        );
        let done = if args.max_run_time > 0 {
            match tokio::time::timeout(Duration::from_secs(args.max_run_time), run).await {
                Ok(result) => result?,
                Err(_) => {
                    info!("max-run-time reached; partial progress saved");
                    0
                }
            }
        } else {
            run.await?
        };
        info!(
            hashed = done,
            remaining = state.lock().await.missing_hashes(),
            "hash done"
        );
    }

    if let Some(out_dir) = args.output {
        let locked = state.lock().await;
        let (extensions, assets) = export::export(&locked, &out_dir)?;
        info!(extensions, assets, "export done");
    }

    Ok(())
}
