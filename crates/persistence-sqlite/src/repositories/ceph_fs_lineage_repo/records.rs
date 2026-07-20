#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsDerivedLineageRecord {
    pub derived_data_source_id: String,
    pub parent_cluster_id: String,
    pub cluster_identity: String,
    pub filesystem_identity: String,
    pub filesystem_id: i64,
    pub filesystem_name: String,
    pub fsmap_epoch: u32,
    pub mdsmap_epoch: u32,
    pub descriptor_state: String,
    pub metadata_pool_id: i64,
    pub expected_replica_count: u32,
    pub namespace_input_sha256: String,
    pub namespace_projection_sha256: String,
    pub namespace_assembly_sha256: String,
    pub source_capability: String,
    pub namespace_schema_version: u32,
    pub decoder_profile: String,
    pub journal_boundary_sha256: Option<String>,
    pub lineage_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsDerivedPoolSourceRecord {
    pub ordinal: u32,
    pub source_data_source_id: String,
    pub inventory_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsDerivedPoolRecord {
    pub pool_id: i64,
    pub role: String,
    pub ordinal: u32,
    pub sources: Vec<CephFsDerivedPoolSourceRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsDerivedMapProvenanceRecord {
    pub ordinal: u32,
    pub source_data_source_id: String,
    pub inventory_id: String,
    pub captured_at: String,
    pub raw_fsmap_sha256: String,
    pub raw_mdsmap_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsDerivedLineageAggregate {
    pub lineage: CephFsDerivedLineageRecord,
    pub pools: Vec<CephFsDerivedPoolRecord>,
    pub map_provenance: Vec<CephFsDerivedMapProvenanceRecord>,
}
