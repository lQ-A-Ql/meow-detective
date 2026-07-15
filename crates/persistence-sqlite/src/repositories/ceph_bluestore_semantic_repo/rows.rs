#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluestoreSemanticScanRecord {
    pub inventory_id: String,
    pub schema_version: u32,
    pub decode_profile: String,
    pub sharding_sha256: String,
    pub latest_state_sha256: String,
    pub semantic_sha256: String,
    pub s_latest_count: u64,
    pub s_decoded_count: u64,
    pub s_deferred_count: u64,
    pub c_latest_count: u64,
    pub c_decoded_count: u64,
    pub c_deferred_count: u64,
    pub o_latest_count: u64,
    pub o_decoded_count: u64,
    pub o_deferred_count: u64,
    pub x_latest_count: u64,
    pub x_decoded_count: u64,
    pub x_deferred_count: u64,
    pub collection_count: u64,
    pub object_count: u64,
    pub blob_count: u64,
    pub onode_shard_count: u64,
    pub logical_extent_count: u64,
    pub physical_extent_count: u64,
    pub checksum_chunk_count: u64,
    pub shared_blob_count: u64,
    pub shared_ref_extent_count: u64,
    pub profile_complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluestoreSuperRecord {
    pub inventory_id: String,
    pub nid_max: Option<u64>,
    pub blobid_max: Option<u64>,
    pub min_alloc_size: Option<u64>,
    pub ondisk_format: Option<i32>,
    pub min_compat_ondisk_format: Option<i32>,
    pub per_pool_omap: Option<String>,
    pub freelist_type: Option<String>,
    pub observed_count: u64,
    pub deferred_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluestoreCollectionRecord {
    pub inventory_id: String,
    pub collection_identity: String,
    pub kind: String,
    pub pool: Option<u64>,
    pub seed: Option<u32>,
    pub shard: Option<u8>,
    pub bits: Option<u32>,
    pub denc_version: Option<u8>,
    pub decode_status: String,
    pub deferred_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluestoreObjectRecord {
    pub inventory_id: String,
    pub object_identity_sha256: String,
    pub decoded_shard: i8,
    pub decoded_pool: i64,
    pub decoded_hash: u32,
    pub decoded_bitwise_hash: u32,
    pub object_namespace: Vec<u8>,
    pub object_key: Option<Vec<u8>>,
    pub object_name: Vec<u8>,
    pub snap_hex: String,
    pub generation_hex: String,
    pub onode_denc_version: u8,
    pub nid: u64,
    pub size: u64,
    pub flags_raw: u8,
    pub flag_omap: bool,
    pub flag_pgmeta_omap: bool,
    pub flag_per_pool_omap: bool,
    pub flag_per_pg_omap: bool,
    pub flags_unknown_bits: u8,
    pub attribute_count: u64,
    pub attribute_value_bytes: u64,
    pub attributes_sha256: String,
    pub expected_object_size: u64,
    pub expected_write_size: u64,
    pub allocation_hint_flags: u32,
    pub zone_ref_count: u64,
    pub extent_storage: String,
    pub spanning_blob_version: u8,
    pub declared_spanning_blob_count: u64,
    pub decode_status: String,
    pub deferred_reason: Option<String>,
    pub onode_shard_count: u64,
    pub blob_count: u64,
    pub logical_extent_count: u64,
    pub physical_extent_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluestoreOnodeShardRecord {
    pub inventory_id: String,
    pub object_identity_sha256: String,
    pub shard_ordinal: u32,
    pub shard_offset: u32,
    pub descriptor_bytes: u32,
    pub payload_version: Option<u8>,
    pub declared_extent_count: Option<u64>,
    pub payload_encoded_length: Option<u64>,
    pub decode_status: String,
    pub deferred_reason: Option<String>,
    pub logical_extent_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluestoreBlobRecord {
    pub inventory_id: String,
    pub object_identity_sha256: String,
    pub blob_ordinal: u32,
    pub blob_kind: String,
    pub blob_id_hex: String,
    pub shared_blob_id_hex: Option<String>,
    pub logical_length: u64,
    pub on_disk_length: u64,
    pub compressed_length: Option<u64>,
    pub flags_raw: u32,
    pub flag_legacy_mutable: bool,
    pub flag_compressed: bool,
    pub flag_checksum: bool,
    pub flag_has_unused: bool,
    pub flag_shared: bool,
    pub flags_unknown_bits: u32,
    pub unused_bitmap: Option<u16>,
    pub checksum_type: Option<String>,
    pub checksum_order: Option<u8>,
    pub checksum_chunk_size: Option<u64>,
    pub checksum_encoded_length: Option<u64>,
    pub checksum_value_count: u64,
    pub checksum_data_crc32c: Option<u32>,
    pub checksum_digest_sha256: Option<String>,
    pub use_tracker_kind: Option<String>,
    pub use_tracker_allocation_unit_size: Option<u64>,
    pub use_tracker_declared_allocation_units: Option<u64>,
    pub use_tracker_entry_count: u64,
    pub use_tracker_sha256: Option<String>,
    pub logical_extent_count: u64,
    pub physical_extent_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluestoreLogicalExtentRecord {
    pub inventory_id: String,
    pub object_identity_sha256: String,
    pub extent_ordinal: u32,
    pub logical_offset: u64,
    pub length: u64,
    pub blob_ordinal: u32,
    pub blob_offset: u64,
    pub shard_ordinal: Option<u32>,
    pub defines_blob: bool,
    pub flags_raw: u8,
    pub flag_contiguous: bool,
    pub flag_zero_blob_offset: bool,
    pub flag_same_length: bool,
    pub flag_spanning: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluestorePhysicalExtentRecord {
    pub inventory_id: String,
    pub object_identity_sha256: String,
    pub blob_ordinal: u32,
    pub extent_ordinal: u32,
    pub blob_offset: u64,
    pub device_id: u8,
    pub physical_offset_hex: Option<String>,
    pub length: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluestoreChecksumChunkRecord {
    pub inventory_id: Arc<str>,
    pub object_identity_sha256: Arc<str>,
    pub blob_ordinal: u32,
    pub checksum_ordinal: u32,
    pub chunk_offset: u64,
    pub chunk_length: u64,
    pub checksum_value_hex: Box<str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluestoreSharedBlobRecord {
    pub inventory_id: String,
    pub shared_blob_id_hex: String,
    pub denc_version: Option<u8>,
    pub decode_status: String,
    pub deferred_reason: Option<String>,
    pub ref_extent_count: u64,
    pub total_ref_bytes: u64,
    pub total_refs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluestoreSharedBlobRefRecord {
    pub inventory_id: String,
    pub shared_blob_id_hex: String,
    pub ref_ordinal: u32,
    pub ref_offset_hex: String,
    pub length: u64,
    pub refs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluestoreSemanticAggregate {
    pub scan: CephBluestoreSemanticScanRecord,
    pub super_record: CephBluestoreSuperRecord,
    pub collections: Vec<CephBluestoreCollectionRecord>,
    pub objects: Vec<CephBluestoreObjectRecord>,
    pub onode_shards: Vec<CephBluestoreOnodeShardRecord>,
    pub blobs: Vec<CephBluestoreBlobRecord>,
    pub logical_extents: Vec<CephBluestoreLogicalExtentRecord>,
    pub physical_extents: Vec<CephBluestorePhysicalExtentRecord>,
    pub checksum_chunks: Vec<CephBluestoreChecksumChunkRecord>,
    pub shared_blobs: Vec<CephBluestoreSharedBlobRecord>,
    pub shared_blob_refs: Vec<CephBluestoreSharedBlobRefRecord>,
}
use std::sync::Arc;
