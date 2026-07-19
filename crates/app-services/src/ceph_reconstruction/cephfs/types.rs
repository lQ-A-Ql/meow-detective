use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsMapEvidence {
    pub cluster_identity: String,
    pub source_identity: String,
    pub inventory_identity: String,
    pub captured_at: DateTime<Utc>,
    pub map: ceph_wire::CephFsMap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsPoolEvidence {
    pub pool_id: i64,
    pub cluster_identity: String,
    pub source_identity: String,
    pub inventory_identity: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsDescriptor {
    pub identity: String,
    pub cluster_identity: String,
    pub filesystem_id: i64,
    pub name: String,
    pub fsmap_epoch: u32,
    pub mdsmap_epoch: u32,
    pub state: CephFsDescriptorState,
    pub metadata_pool: CephFsPoolBinding,
    pub data_pools: Vec<CephFsPoolBinding>,
    pub daemons: Vec<ceph_wire::CephMdsDaemon>,
    pub provenance: Vec<CephFsMapProvenance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CephFsDescriptorState {
    Present,
    PresentButNotReplayable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsPoolBinding {
    pub pool_id: i64,
    pub role: CephFsPoolRole,
    pub provenance: Vec<CephFsPoolProvenance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CephFsPoolRole {
    Metadata,
    Data { ordinal: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CephFsMapProvenance {
    pub source_identity: String,
    pub inventory_identity: String,
    pub captured_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CephFsPoolProvenance {
    pub source_identity: String,
    pub inventory_identity: String,
}
