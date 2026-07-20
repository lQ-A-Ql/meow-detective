pub const CEPHFS_NAMESPACE_SCHEMA_VERSION: u32 = 1;
pub const CEPHFS_NAMESPACE_DECODER_PROFILE: &str = "cephfs-namespace-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsNamespaceManifest {
    pub filesystem_identity: String,
    pub data_source_id: String,
    pub filesystem_id: i64,
    pub fsmap_epoch: u32,
    pub root_inode: u64,
    pub input_sha256: String,
    pub projection_sha256: String,
    pub schema_version: u32,
    pub decoder_profile: String,
    pub completeness: String,
    pub published: bool,
    pub entry_count: u64,
    pub inode_count: u64,
    pub diagnostic_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsInodeRecord {
    pub inode: u64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub nlink: i32,
    pub size: u64,
    pub inode_kind: String,
    pub encoded_version: u8,
    pub remaining_inode_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsFileLayoutRecord {
    pub inode: u64,
    pub stripe_unit: u32,
    pub stripe_count: u32,
    pub object_size: u32,
    pub pool_id: i64,
    pub pool_namespace: String,
    pub inline_data: Option<Vec<u8>>,
    pub sparse_extents: Vec<CephFsSparseExtentRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsSparseExtentRecord {
    pub offset: u64,
    pub length: u64,
    pub evidence_sha256: String,
    pub proof_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsDentryRecord {
    pub entry_id: String,
    pub parent_entry_id: Option<String>,
    pub parent_inode: u64,
    pub child_inode: u64,
    pub fragment: u32,
    pub name: String,
    pub path: String,
    pub entry_kind: String,
    pub mode: Option<u32>,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub nlink: Option<i32>,
    pub size: Option<u64>,
    pub alternate_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsNamespaceDiagnosticRecord {
    pub diagnostic_ordinal: u64,
    pub diagnostic_kind: String,
    pub parent_inode: u64,
    pub child_inode: u64,
    pub name: String,
    pub snap_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsNamespaceProjection {
    pub manifest: CephFsNamespaceManifest,
    pub inodes: Vec<CephFsInodeRecord>,
    pub layouts: Vec<CephFsFileLayoutRecord>,
    pub dentries: Vec<CephFsDentryRecord>,
    pub diagnostics: Vec<CephFsNamespaceDiagnosticRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsFileLocatorRecord {
    pub filesystem_identity: String,
    pub data_source_id: String,
    pub filesystem_id: i64,
    pub fsmap_epoch: u32,
    pub projection_sha256: String,
    pub schema_version: u32,
    pub decoder_profile: String,
    pub entry_id: String,
    pub inode: u64,
    pub entry_kind: String,
    pub size: u64,
    pub stripe_unit: u32,
    pub stripe_count: u32,
    pub object_size: u32,
    pub pool_id: i64,
    pub pool_namespace: String,
    pub inline_data: Option<Vec<u8>>,
    pub sparse_extents: Vec<CephFsSparseExtentRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CephFsFileCatalogSummary {
    pub file_count: u64,
    pub directory_count: u64,
    pub total_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsPublishedCatalog {
    pub manifest: CephFsNamespaceManifest,
    pub summary: CephFsFileCatalogSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CephFsNamespaceWriteOutcome {
    Replaced,
    Unchanged,
}
