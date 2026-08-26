//! Enumeration phase: paginate `/extension-query/` until an empty page.

use anyhow::Result;
use tracing::{info, warn};

use crate::{api::GnomeClient, state::Row, state::State};

/// Safety valve against runaway pagination (numpages is unreliable upstream).
const HARD_PAGE_CAP: u64 = 5000;

pub async fn fetch_extensions(
    client: &GnomeClient,
    state: &mut State,
    sort: &str,
    max_pages: u64,
) -> Result<(usize, usize)> {
    let mut page: u64 = 1;
    let mut extensions_seen = 0usize;
    let mut rows_added = 0usize;

    loop {
        if page > HARD_PAGE_CAP {
            warn!(page, "hard page cap reached, stopping enumeration");
            break;
        }
        if max_pages > 0 && page > max_pages {
            info!(page, "max-pages limit reached");
            break;
        }

        let resp = client.query_page(sort, page).await?;
        if page == 1 {
            info!(
                sort,
                total = resp.total,
                numpages = resp.numpages,
                "enumeration started"
            );
        }
        if resp.extensions.is_empty() {
            info!(page, "empty page, enumeration complete");
            break;
        }

        for ext in &resp.extensions {
            extensions_seen += 1;
            for (shell, ver) in &ext.shell_version_map {
                let row = state.rows.entry(ver.pk).or_insert_with(|| {
                    rows_added += 1;
                    Row {
                        uuid: ext.uuid.clone(),
                        version: ver.version,
                        shells: Default::default(),
                        hash: None,
                    }
                });
                if row.uuid != ext.uuid {
                    warn!(
                        tag = ver.pk,
                        existing = %row.uuid,
                        incoming = %ext.uuid,
                        "version tag shared by different extensions; keeping first"
                    );
                    continue;
                }
                row.version = ver.version;
                row.shells.insert(shell.clone());
            }
        }

        if page.is_multiple_of(25) {
            info!(page, assets = state.rows.len(), "enumerating");
        }
        page += 1;
    }

    Ok((extensions_seen, rows_added))
}
