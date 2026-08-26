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

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::Result;
use serde::Serialize;
use tracing::info;

use crate::state::State;

const SHARDS: usize = 16;

#[derive(Debug, Serialize)]
struct ExportedEntry {
    v: u64,
    t: u64,
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

pub fn export(state: &State, out_dir: &Path) -> Result<(usize, usize)> {
    let mut by_uuid: BTreeMap<String, Vec<ExportedEntry>> = BTreeMap::new();
    let mut assets = 0usize;

    for (tag, row) in &state.rows {
        let Some(hash) = row.hash.as_deref().filter(|h| !h.is_empty()) else {
            continue;
        };
        assets += 1;
        by_uuid
            .entry(row.uuid.clone())
            .or_default()
            .push(ExportedEntry {
                v: row.version,
                t: *tag,
                h: hash.to_owned(),
                s: row.shells.iter().cloned().collect(),
            });
    }

    for entries in by_uuid.values_mut() {
        entries.sort_by(|a, b| b.v.cmp(&a.v).then(b.t.cmp(&a.t)));
    }

    fs::create_dir_all(out_dir)?;
    let mut shards: Vec<BTreeMap<&str, &Vec<ExportedEntry>>> = vec![BTreeMap::new(); SHARDS];
    for (uuid, entries) in &by_uuid {
        let idx = fnv1a(uuid) as usize % SHARDS;
        shards[idx].insert(uuid.as_str(), entries);
    }

    for (idx, shard) in shards.iter().enumerate() {
        let json = serde_json::to_string(shard)?;
        fs::write(out_dir.join(format!("data_{idx}.json")), json)?;
    }

    info!(extensions = by_uuid.len(), assets, dir = %out_dir.display(), "export written");
    Ok((by_uuid.len(), assets))
}
