//! SQLite state database: connection handling, schema bootstrap, compaction.

use std::path::Path;

use anyhow::{Context, Result};
use diesel::{
    connection::SimpleConnection, deserialize::QueryableByName, prelude::*, sql_query,
    sql_types::BigInt, SqliteConnection,
};

pub fn establish(path: &Path) -> Result<SqliteConnection> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    SqliteConnection::establish(&path.display().to_string())
        .with_context(|| format!("open database {}", path.display()))
}

pub fn ensure_schema(conn: &mut SqliteConnection) -> Result<()> {
    conn.batch_execute(
        "CREATE TABLE IF NOT EXISTS assets (
            version_tag INTEGER PRIMARY KEY,
            uuid TEXT NOT NULL,
            version INTEGER NOT NULL,
            hash TEXT
        );
        CREATE TABLE IF NOT EXISTS asset_shells (
            version_tag INTEGER NOT NULL,
            shell TEXT NOT NULL,
            PRIMARY KEY (version_tag, shell)
        );",
    )
    .context("create schema")?;
    Ok(())
}

#[derive(Debug, QueryableByName)]
struct Count {
    #[diesel(sql_type = BigInt)]
    n: i64,
}

pub fn asset_count(conn: &mut SqliteConnection) -> Result<i64> {
    let count: Count = sql_query("SELECT COUNT(*) AS n FROM assets").get_result(conn)?;
    Ok(count.n)
}

/// Reclaim space before the database gets packed for the db branch.
pub fn vacuum(conn: &mut SqliteConnection) -> Result<()> {
    conn.batch_execute("VACUUM;").context("vacuum")?;
    Ok(())
}
