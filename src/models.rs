use diesel::prelude::*;

use crate::schema::{asset_shells, assets};

/// One downloadable asset zip, identified by its upstream version tag.
#[derive(Debug, Clone, Queryable, Identifiable)]
#[diesel(table_name = assets)]
#[diesel(primary_key(version_tag))]
pub struct Asset {
    pub version_tag: i32,
    pub uuid: String,
    pub version: i32,
    pub hash: Option<String>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = assets)]
pub struct NewAsset<'a> {
    pub version_tag: i32,
    pub uuid: &'a str,
    pub version: i32,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = asset_shells)]
pub struct NewShell<'a> {
    pub version_tag: i32,
    pub shell: &'a str,
}
