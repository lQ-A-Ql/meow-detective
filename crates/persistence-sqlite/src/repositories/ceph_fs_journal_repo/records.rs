pub const CEPHFS_JOURNAL_SCHEMA_VERSION: u32 = 1;
pub const CEPHFS_JOURNAL_DECODER_PROFILE: &str = "cephfs-journal-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsJournalReplayManifest {
    pub filesystem_identity: String,
    pub inventory_id: String,
    pub data_source_id: String,
    pub rank: u32,
    pub filesystem_id: i64,
    pub fsmap_epoch: u32,
    pub mdsmap_epoch: u32,
    pub rank_incarnation: i32,
    pub rank_gid_hex: String,
    pub pointer_front_inode_hex: String,
    pub pointer_back_inode_hex: String,
    pub journal_inode_hex: String,
    pub schema_version: u32,
    pub decoder_profile: String,
    pub source_semantic_sha256: String,
    pub metadata_inventory_sha256: String,
    pub raw_fsmap_snapshot_sha256: String,
    pub raw_mdsmap_snapshot_sha256: String,
    pub map_provenance_sha256: String,
    pub map_provenance_count: u64,
    pub pointer_locator: String,
    pub pointer_object_identity_sha256: String,
    pub pointer_range_offset_hex: String,
    pub pointer_range_length_hex: String,
    pub pointer_range_sha256: String,
    pub header_locator: String,
    pub header_object_identity_sha256: String,
    pub header_range_offset_hex: String,
    pub header_range_length_hex: String,
    pub header_range_sha256: String,
    pub trimmed_pos_hex: String,
    pub expire_pos_hex: String,
    pub unused_pos_hex: String,
    pub write_pos_hex: String,
    pub committed_header_tail_hex: String,
    pub framing_safe_pos_hex: String,
    pub namespace_safe_pos_hex: Option<String>,
    pub sequence_safe_pos_hex: String,
    pub stream_format: String,
    pub framing_status: String,
    pub stop_reason: Option<String>,
    pub namespace_stop_reason: Option<String>,
    pub sequence_stop_reason: Option<String>,
    pub event_count: u64,
    pub input_sha256: String,
    pub consensus_replay_sha256: String,
    pub projection_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsJournalMapProvenanceRecord {
    pub filesystem_identity: String,
    pub inventory_id: String,
    pub rank: u32,
    pub source_identity: String,
    pub source_inventory_identity: String,
    pub captured_at: String,
    pub raw_fsmap_snapshot_sha256: String,
    pub raw_mdsmap_snapshot_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsJournalEventRecord {
    pub filesystem_identity: String,
    pub inventory_id: String,
    pub rank: u32,
    pub event_ordinal: u64,
    pub segment_sequence_hex: Option<String>,
    pub event_sequence_hex: Option<String>,
    pub sequence_disposition: String,
    pub logical_offset_hex: String,
    pub logical_end_hex: String,
    pub payload_length: u32,
    pub payload_sha256: String,
    pub event_type: u32,
    pub event_kind: String,
    pub event_encoding: String,
    pub event_version: Option<u8>,
    pub event_compat_version: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsJournalEventSpanRecord {
    pub filesystem_identity: String,
    pub inventory_id: String,
    pub rank: u32,
    pub event_ordinal: u64,
    pub span_ordinal: u64,
    pub object_locator: String,
    pub object_identity_sha256: String,
    pub logical_offset_hex: String,
    pub object_offset_hex: String,
    pub range_length_hex: String,
    pub range_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsJournalReplayProjection {
    pub manifest: CephFsJournalReplayManifest,
    pub map_provenance: Vec<CephFsJournalMapProvenanceRecord>,
    pub events: Vec<CephFsJournalEventRecord>,
    pub spans: Vec<CephFsJournalEventSpanRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CephFsJournalWriteOutcome {
    Replaced,
    Unchanged,
}

pub fn cephfs_journal_u64_hex(value: u64) -> String {
    format!("{value:016x}")
}
