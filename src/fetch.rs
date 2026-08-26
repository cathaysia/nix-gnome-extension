//! Enumeration phase: paginate `/extension-query/` until an empty page,
//! upserting assets and their shell compatibility rows into SQLite.

use std::time::Duration;

use anyhow::Result;
use diesel::prelude::*;
use tracing::{info, warn};

use crate::{
    api::GnomeClient,
    models::{Asset, NewAsset, NewShell},
    schema::{asset_shells, assets},
};

/// Safety valve against runaway pagination (numpages is unreliable upstream).
const HARD_PAGE_CAP: u64 = 5000;

pub async fn fetch_extensions(
    client: &GnomeClient,
    conn: &mut SqliteConnection,
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

        let extensions = &resp.extensions;
        conn.transaction::<_, diesel::result::Error, _>(|tx| {
            for ext in extensions {
                extensions_seen += 1;
                for (shell, ver) in &ext.shell_version_map {
                    let Ok(tag) = i32::try_from(ver.pk) else {
                        warn!(pk = ver.pk, uuid = %ext.uuid, "version tag out of range");
                        continue;
                    };
                    let existing: Option<Asset> = assets::table.find(tag).first(tx).optional()?;
                    match existing {
                        None => {
                            diesel::insert_into(assets::table)
                                .values(NewAsset {
                                    version_tag: tag,
                                    uuid: &ext.uuid,
                                    version: i32::try_from(ver.version).unwrap_or(i32::MAX),
                                })
                                .on_conflict_do_nothing()
                                .execute(tx)?;
                            rows_added += 1;
                        }
                        Some(ref asset) if asset.uuid != ext.uuid => {
                            warn!(
                                tag,
                                kept = %asset.uuid,
                                seen = %ext.uuid,
                                "version tag shared by different extensions; keeping first"
                            );
                            continue;
                        }
                        Some(_) => {}
                    }
                    diesel::insert_into(asset_shells::table)
                        .values(NewShell {
                            version_tag: tag,
                            shell,
                        })
                        .on_conflict_do_nothing()
                        .execute(tx)?;
                }
            }
            Ok(())
        })?;

        if page.is_multiple_of(25) {
            let count: i64 = assets::table
                .select(diesel::dsl::count_star())
                .get_result(conn)?;
            info!(page, assets = count, "enumerating");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        page += 1;
    }

    Ok((extensions_seen, rows_added))
}
