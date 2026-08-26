mod api;
mod db;
mod export;
mod fetch;
mod hash;
mod mini_json;
mod models;
mod nix_hash;
mod schema;

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
    /// Fetch extension list from extensions.gnome.org into the state database.
    #[arg(long)]
    fetch: bool,

    /// Compute hashes for assets that do not have one yet.
    #[arg(long)]
    hash: bool,

    /// Directory to write the sharded JSON data files to.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// SQLite state database path.
    #[arg(long, default_value = "db.sqlite3")]
    db: PathBuf,

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
    let mut conn = db::establish(&args.db)?;
    db::ensure_schema(&mut conn)?;
    info!(
        db = %args.db.display(),
        assets = db::asset_count(&mut conn)?,
        "database ready"
    );

    if args.fetch {
        let (extensions, added) =
            fetch::fetch_extensions(&client, &mut conn, &args.sort, args.max_pages).await?;
        info!(
            extensions,
            added,
            assets = db::asset_count(&mut conn)?,
            "fetch done"
        );
    }

    if args.hash {
        conn = {
            let shared = Arc::new(Mutex::new(conn));
            let run = hash::fetch_hash(&client, Arc::clone(&shared), args.batch_size, args.limit);
            let done = if args.max_run_time > 0 {
                match tokio::time::timeout(Duration::from_secs(args.max_run_time), run).await {
                    Ok(result) => result?,
                    Err(_) => {
                        info!("max-run-time reached; partial progress persisted");
                        0
                    }
                }
            } else {
                run.await?
            };
            let mut owned = Arc::try_unwrap(shared)
                .map_err(|_| ())
                .expect("connection uniquely held after hashing")
                .into_inner();
            info!(
                hashed = done,
                remaining = hash::missing_count(&mut owned)?,
                "hash done"
            );
            owned
        };
    }

    if args.output.is_some() || args.fetch || args.hash {
        // Compact before CI packs the database for the db branch.
        db::vacuum(&mut conn)?;
    }

    if let Some(out_dir) = args.output {
        let (extensions, assets) = export::export(&mut conn, &out_dir)?;
        info!(extensions, assets, "export done");
    }

    Ok(())
}
