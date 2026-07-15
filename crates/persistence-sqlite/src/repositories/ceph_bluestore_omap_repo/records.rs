#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluestoreOmapScanRecord {
    pub inventory_id: String,
    pub data_source_id: String,
    pub schema_version: u32,
    pub decode_profile: String,
    pub sharding_sha256: String,
    pub latest_state_sha256: String,
    pub semantic_sha256: String,
    pub omap_sha256: String,
    pub scope_count: u64,
    pub directory_mapping_count: u64,
    pub rbd_header_count: u64,
    pub profile_complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluestoreOmapScopeRecord {
    pub inventory_id: String,
    pub scope_identity: String,
    pub key_family: String,
    pub pool_kind: String,
    pub pool_value_i64: Option<i64>,
    pub pool_value_hex: Option<String>,
    pub hash: Option<u32>,
    pub nid_hex: String,
    pub owner_nid_hex: Option<String>,
    pub owner_family: Option<String>,
    pub owner_kind: Option<String>,
    pub owner_image_id: Option<String>,
    pub entry_count: u64,
    pub recognized_entry_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluestoreRbdDirectoryRecord {
    pub inventory_id: String,
    pub scope_identity: String,
    pub owner_nid_hex: String,
    pub image_name: String,
    pub image_id: String,
    pub bidirectional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluestoreRbdHeaderRecord {
    pub inventory_id: String,
    pub scope_identity: String,
    pub owner_nid_hex: String,
    pub image_id: String,
    pub size_hex: Option<String>,
    pub object_order: Option<u8>,
    pub features_hex: Option<String>,
    pub operation_features_hex: Option<String>,
    pub parent_key_present: bool,
    pub object_prefix: Option<String>,
    pub stripe_unit_hex: Option<String>,
    pub stripe_count_hex: Option<String>,
    pub data_pool_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluestoreOmapAggregate {
    pub scan: CephBluestoreOmapScanRecord,
    pub scopes: Vec<CephBluestoreOmapScopeRecord>,
    pub directory_mappings: Vec<CephBluestoreRbdDirectoryRecord>,
    pub rbd_headers: Vec<CephBluestoreRbdHeaderRecord>,
}
