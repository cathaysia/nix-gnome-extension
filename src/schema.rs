diesel::table! {
    assets (version_tag) {
        version_tag -> Integer,
        uuid -> Text,
        version -> Integer,
        hash -> Nullable<Text>,
    }
}

diesel::table! {
    asset_shells (version_tag, shell) {
        version_tag -> Integer,
        shell -> Text,
    }
}

diesel::allow_tables_to_appear_in_same_query!(assets, asset_shells);
