pub const CEPHFS_METADATA_SCHEMA_VERSION: u32 = 1;
pub const CEPHFS_METADATA_CLASSIFIER_PROFILE: &str = "cephfs-metadata-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsMetadataInventoryManifest {
    pub filesystem_identity: String,
    pub inventory_id: String,
    pub data_source_id: String,
    pub filesystem_id: i64,
    pub fsmap_epoch: u32,
    pub metadata_pool_id: i64,
    pub schema_version: u32,
    pub classifier_profile: String,
    pub source_semantic_sha256: String,
    pub inventory_sha256: String,
    pub object_count: u64,
    pub unknown_object_count: u64,
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsMetadataObjectProjection {
    pub object_identity_sha256: String,
    pub locator: String,
    pub candidate_mask: u8,
    pub classification_state: String,
    pub classifier_rule: String,
    pub record_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsMetadataInventory {
    pub manifest: CephFsMetadataInventoryManifest,
    pub objects: Vec<CephFsMetadataObjectProjection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CephFsMetadataWriteOutcome {
    Replaced,
    Unchanged,
}
