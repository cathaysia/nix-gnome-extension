//! Resumable on-disk state: one row per unique asset (version tag).
//!
//! Kept as plain JSON instead of SQLite (nix4vscode uses diesel+sqlite) to
//! keep compile times low; the dataset (~7k assets) fits comfortably.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    /// Keyed by version tag (the `pk` inside `shell_version_map`).
    pub rows: BTreeMap<u64, Row>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Row {
    pub uuid: String,
    pub version: u64,
    /// All GNOME Shell major versions served by this exact asset.
    #[serde(default)]
    pub shells: BTreeSet<String>,
    /// Nix base32 SHA-256, set once computed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
}

impl State {
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read(path) {
            Ok(data) => serde_json::from_slice(&data)
                .with_context(|| format!("parse state file {}", path.display())),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(err).context(format!("read state file {}", path.display())),
        }
    }

    /// Atomic write: temp file + rename.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec(self)?)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn missing_hashes(&self) -> usize {
        self.rows
            .values()
            .filter(|row| row.hash.as_deref().is_none_or(str::is_empty))
            .count()
    }
}
