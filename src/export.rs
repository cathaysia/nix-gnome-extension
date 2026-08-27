//! Export phase: emit deterministic sharded JSON consumed by the Nix library.
//!
//! Schema per shard (map, one version object per line):
//!
//! ```json
//! {
//! "uuid": [
//! {"v":72,"t":69740,"h":"<nix base32 sha256>","s":["46","47"]}
//! ]
//! }
//! ```
//!
//! - `v`: extension version number, `t`: version tag,
//!   `h`: Nix base32 SHA-256, `s`: compatible GNOME Shell versions.
//! - Shards assigned by FNV-1a so output is byte-stable across runs.

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

#[derive(Debug, Clone, Serialize)]
struct VersionEntry {
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

    let mut shards: Vec<BTreeMap<String, Vec<VersionEntry>>> = vec![BTreeMap::new(); SHARDS];
    let mut extensions_set: BTreeSet<String> = BTreeSet::new();
    for asset in hashed {
        let Some(hash) = asset.hash.filter(|h| !h.is_empty()) else {
            continue;
        };
        let shells = shells_by_tag
            .get(&asset.version_tag)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default();
        let idx = fnv1a(&asset.uuid) as usize % SHARDS;
        extensions_set.insert(asset.uuid.clone());
        shards[idx]
            .entry(asset.uuid)
            .or_default()
            .push(VersionEntry {
                v: asset.version,
                t: asset.version_tag,
                h: hash,
                s: shells,
            });
    }

    for shard in &mut shards {
        for entries in shard.values_mut() {
            entries.sort_by(|a, b| b.v.cmp(&a.v).then(b.t.cmp(&a.t)));
        }
    }
    let assets_total: usize = shards
        .iter()
        .map(|m| m.values().map(Vec::len).sum::<usize>())
        .sum();

    // Use compact per-object serialization to keep one version object per line,
    // while still using the ported mini_json module for other cases.
    // Manual here ensures `s` stays compact `["46","47"]` instead of being
    // expanded one string per line by mini_json's generic seq handling.
    fs::create_dir_all(out_dir)?;
    for (idx, shard) in shards.iter().enumerate() {
        let json = if shard.is_empty() {
            "{}".to_string()
        } else {
            let mut out = String::from("{\n");
            for (i, (uuid, entries)) in shard.iter().enumerate() {
                out.push_str(&format!("\"{}\": [\n", uuid));
                for (j, entry) in entries.iter().enumerate() {
                    out.push_str(&serde_json::to_string(entry).unwrap());
                    if j + 1 < entries.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                out.push(']');
                if i + 1 < shard.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push('}');
            out
        };
        fs::write(out_dir.join(format!("data_{idx}.json")), json)?;
    }

    info!(extensions = extensions_set.len(), assets = assets_total, dir = %out_dir.display(), "export written");
    Ok((extensions_set.len(), assets_total))
}
