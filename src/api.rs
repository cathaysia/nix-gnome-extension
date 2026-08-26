//! HTTP client and models for the extensions.gnome.org JSON API.
//!
//! Verified endpoints (2026-08):
//!
//! - `GET /extension-query/?search=<q>&sort=<name|downloads|recent>&page=<n>`
//!   Returns `{extensions: [...], total, numpages}` with a fixed page size of 10.
//!   Each item carries `shell_version_map: {"46": {pk, version}, ...}` where the
//!   inner `pk` is the *version tag* needed for the download URL.
//!
//! - `GET /download-extension/<uuid>.shell-extension.zip?version_tag=<pk>`
//!   302-redirects to `/api/v1/extensions/<uuid>/versions/<v>/?format=zip`.
//!
//! The REST API under `/api/v1/` exists (`/api/v1/extensions/?page_size=100`,
//! count 5154) but its versions endpoint lacks download URLs, so enumeration
//! goes through `/extension-query/` which carries everything we need.

use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;

const BASE_URL: &str = "https://extensions.gnome.org";
const MAX_RETRIES: usize = 4;

#[derive(Debug, Deserialize)]
pub struct QueryResponse {
    #[serde(default)]
    pub extensions: Vec<ExtensionSummary>,
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub numpages: u64,
}

/// Fields not consumed by the exporter are kept to document the upstream
/// API surface.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct ExtensionSummary {
    pub uuid: String,
    pub pk: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub shell_version_map: HashMap<String, VersionRef>,
}

/// Reference to the specific asset version serving a given GNOME Shell version.
///
/// `pk` is the version tag used in download URLs; `version` is the human
/// visible extension version number.
#[derive(Debug, Deserialize)]
pub struct VersionRef {
    pub pk: u64,
    pub version: u64,
}

pub struct GnomeClient {
    http: Client,
}

impl GnomeClient {
    pub fn new() -> Result<Self> {
        let http = Client::builder()
            .user_agent(concat!(
                "nix-gnome-extension-exporter/",
                env!("CARGO_PKG_VERSION")
            ))
            .timeout(Duration::from_secs(120))
            .build()
            .context("build HTTP client")?;
        Ok(Self { http })
    }

    /// GET with retry and exponential backoff (1s, 3s, 9s).
    pub async fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        self.try_get_bytes(url)
            .await?
            .ok_or_else(|| anyhow!("GET {url}: unexpected 404"))
    }

    async fn try_get_bytes(&self, url: &str) -> Result<Option<Vec<u8>>> {
        use reqwest::StatusCode;

        let mut delay = Duration::from_secs(1);
        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                tracing::debug!(url, attempt, "retrying");
                tokio::time::sleep(delay).await;
                delay *= 3;
            }
            let result = async {
                let resp = self.http.get(url).send().await?;
                if resp.status() == StatusCode::NOT_FOUND {
                    return Ok(None);
                }
                let resp = resp.error_for_status()?;
                Ok(Some(resp.bytes().await?.to_vec()))
            }
            .await;
            match result {
                Ok(bytes) => return Ok(bytes),
                Err(err) => last_err = Some(err),
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("GET {url} failed without error")))
            .with_context(|| format!("GET {url}"))
    }

    /// Query one page. Returns `Ok(None)` when the site answers 404, which is
    /// how its paginator signals a page beyond the end (no empty-list page).
    pub async fn query_page(&self, sort: &str, page: u64) -> Result<Option<QueryResponse>> {
        let url = format!("{BASE_URL}/extension-query/?sort={sort}&page={page}");
        let body = match self.try_get_bytes(&url).await? {
            Some(bytes) => bytes,
            None => return Ok(None),
        };
        serde_json::from_slice(&body)
            .map(Some)
            .with_context(|| format!("decode response of {url}"))
    }

    /// Canonical asset URL for a version tag. Follows a 302 to the REST API.
    pub fn download_url(uuid: &str, version_tag: u64) -> String {
        format!(
            "{BASE_URL}/download-extension/{uuid}.shell-extension.zip?version_tag={version_tag}"
        )
    }
}
