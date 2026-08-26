//! Export phase: emit deterministic sharded JSON consumed by the Nix library.
//!
//! Schema per shard:
//!
//! ```json
//! { "<uuid>": [ { "v": 72, "t": 69740, "h": "<nix base32 sha256>", "s": ["46","47"] } ] }
//! ```
//!
//! - `v`: extension version number, `t`: version tag for the download URL,
//!   `h`: Nix base32 SHA-256, `s`: compatible GNOME Shell major versions.
//! - Entries sorted by descending version, keys alphabetical, shards assigned
//!   by FNV-1a so output is byte-stable across runs.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::Path;

use anyhow::Result;
use diesel::prelude::*;
use serde::Serialize;
use tracing::info;

use crate::{
    models::Asset,
    schema::{asset_shells, assets},
};

const SHARDS: usize = 16;

#[derive(Debug, Serialize)]
struct ExportedEntry {
    v: i32,
    t: i32,
    h: String,
    s: Vec<String>,
}

fn fnv1a(value: &str) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in value.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

pub fn export(conn: &mut SqliteConnection, out_dir: &Path) -> Result<(usize, usize)> {
    let hashed: Vec<Asset> = assets::table
        .filter(assets::hash.is_not_null())
        .filter(assets::hash.ne(""))
        .order_by(assets::version_tag)
        .load(conn)?;
    let shell_rows: Vec<(i32, String)> = asset_shells::table
        .order_by((asset_shells::version_tag, asset_shells::shell))
        .load(conn)?;

    let mut shells_by_tag: HashMap<i32, BTreeSet<String>> = HashMap::new();
    for (tag, shell) in shell_rows {
        shells_by_tag.entry(tag).or_default().insert(shell);
    }

    let mut by_uuid: BTreeMap<String, Vec<ExportedEntry>> = BTreeMap::new();
    for asset in hashed {
        let Some(hash) = asset.hash.filter(|h| !h.is_empty()) else {
            continue;
        };
        let shells = shells_by_tag
            .get(&asset.version_tag)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default();
        by_uuid
            .entry(asset.uuid.clone())
            .or_default()
            .push(ExportedEntry {
                v: asset.version,
                t: asset.version_tag,
                h: hash,
                s: shells,
            });
    }

    for entries in by_uuid.values_mut() {
        entries.sort_by(|a, b| b.v.cmp(&a.v).then(b.t.cmp(&a.t)));
    }
    let assets_total: usize = by_uuid.values().map(Vec::len).sum();

    fs::create_dir_all(out_dir)?;
    let mut shards: Vec<BTreeMap<&str, &Vec<ExportedEntry>>> = vec![BTreeMap::new(); SHARDS];
    for (uuid, entries) in &by_uuid {
        let idx = fnv1a(uuid) as usize % SHARDS;
        shards[idx].insert(uuid.as_str(), entries);
    }

    for (idx, shard) in shards.iter().enumerate() {
        let json = crate::mini_json::to_string(shard);
        fs::write(out_dir.join(format!("data_{idx}.json")), json)?;
    }

    info!(extensions = by_uuid.len(), assets = assets_total, dir = %out_dir.display(), "export written");
    Ok((by_uuid.len(), assets_total))
}
