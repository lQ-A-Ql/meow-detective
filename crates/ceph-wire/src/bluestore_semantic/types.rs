use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlueStoreKeySpace {
    Super,
    Collection,
    Object,
    SharedBlob,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlueStoreSemanticLimits {
    pub max_logical_key_bytes: usize,
    pub max_value_bytes: usize,
    pub max_string_bytes: usize,
    pub max_attributes: usize,
    pub max_attribute_value_bytes: usize,
    pub max_total_attribute_value_bytes: usize,
    pub max_extent_shards: usize,
    pub max_zone_refs: usize,
    pub max_spanning_blobs: usize,
    pub max_extent_records: usize,
    pub max_extent_payload_bytes: usize,
    pub max_shared_blob_refs: usize,
    pub max_physical_extents: usize,
    pub max_blobs: usize,
    pub max_checksum_bytes: usize,
    pub max_use_tracker_entries: usize,
    pub max_decoded_heap_bytes: usize,
    pub max_decode_work_units: usize,
}

impl Default for BlueStoreSemanticLimits {
    fn default() -> Self {
        Self {
            max_logical_key_bytes: 64 * 1024,
            max_value_bytes: 64 * 1024 * 1024,
            max_string_bytes: 1024 * 1024,
            max_attributes: 65_536,
            max_attribute_value_bytes: 16 * 1024 * 1024,
            max_total_attribute_value_bytes: 64 * 1024 * 1024,
            max_extent_shards: 1_000_000,
            max_zone_refs: 1_000_000,
            max_spanning_blobs: 1_000_000,
            max_extent_records: 1_000_000,
            max_extent_payload_bytes: 64 * 1024 * 1024,
            max_shared_blob_refs: 1_000_000,
            max_physical_extents: 1_000_000,
            max_blobs: 1_000_000,
            max_checksum_bytes: 64 * 1024 * 1024,
            max_use_tracker_entries: 1_000_000,
            max_decoded_heap_bytes: 128 * 1024 * 1024,
            max_decode_work_units: 256 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlueStoreDecodedRecord {
    Super(BlueStoreSuperRecord),
    Collection(BlueStoreCollectionRecord),
    Object(Box<BlueStoreObjectRecord>),
    SharedBlob(BlueStoreSharedBlobRecord),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlueStoreSuperRecord {
    NidMax(u64),
    BlobIdMax(u64),
    MinAllocSize(u64),
    OndiskFormat(i32),
    MinCompatOndiskFormat(i32),
    PerPoolOmap(BlueStoreOmapMode),
    FreelistType(String),
    Unknown {
        field: String,
        deferred: BlueStoreDeferred,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlueStoreOmapMode {
    Bulk,
    PerPool,
    PerPg,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlueStoreCollectionRecord {
    pub collection: BlueStoreCollectionId,
    pub cnode: BlueStoreCnode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlueStoreCollectionId {
    Meta,
    Pg {
        pool: u64,
        seed: u32,
        shard: Option<u8>,
        kind: BlueStoreCollectionKind,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlueStoreCollectionKind {
    Head,
    Temp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlueStoreCnode {
    pub denc_version: u8,
    pub bits: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlueStoreObjectRecord {
    Onode {
        object: BlueStoreObjectId,
        onode: BlueStoreOnodeHeader,
        tail: BlueStoreOnodeTail,
    },
    ExtentShard {
        object: BlueStoreObjectId,
        shard_offset: u32,
        payload: BlueStoreExtentPayload,
    },
    DeferredExtentShard {
        object: BlueStoreObjectId,
        shard_offset: u32,
        payload: BlueStoreDeferred,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlueStoreObjectKey {
    Onode(BlueStoreObjectId),
    ExtentShard {
        object: BlueStoreObjectId,
        shard_offset: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlueStoreObjectId {
    pub shard: i8,
    pub pool: i64,
    pub hash: u32,
    pub bitwise_hash: u32,
    pub namespace: Vec<u8>,
    pub object_key: Option<Vec<u8>>,
    pub object_name: Vec<u8>,
    pub snap: u64,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlueStoreOnodeHeader {
    pub denc_version: u8,
    pub nid: u64,
    pub size: u64,
    pub attributes: Vec<BlueStoreAttributeSummary>,
    pub flags: BlueStoreOnodeFlags,
    pub extent_shards: Vec<BlueStoreExtentShardDescriptor>,
    pub allocation_hints: BlueStoreAllocationHints,
    pub zone_offset_refs: Vec<BlueStoreZoneOffsetRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlueStoreAttributeSummary {
    pub name: Vec<u8>,
    pub value_length: u32,
    pub value_sha256: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlueStoreOnodeFlags {
    pub raw: u8,
    pub omap: bool,
    pub pgmeta_omap: bool,
    pub per_pool_omap: bool,
    pub per_pg_omap: bool,
    pub unknown_bits: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlueStoreExtentShardDescriptor {
    pub offset: u32,
    pub bytes: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlueStoreAllocationHints {
    pub expected_object_size: u32,
    pub expected_write_size: u32,
    pub flags: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlueStoreZoneOffsetRef {
    pub zone: u32,
    pub offset: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlueStoreOnodeTail {
    Decoded {
        spanning_blob_version: u8,
        spanning_blobs: Vec<BlueStoreBlob>,
        extents: BlueStoreExtentStorage,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlueStoreExtentStorage {
    Inline(BlueStoreExtentPayload),
    Sharded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlueStoreExtentPayload {
    pub version: u8,
    pub declared_extent_count: u32,
    pub encoded_length: usize,
    pub blobs: Vec<BlueStoreBlob>,
    pub extents: Vec<BlueStoreLogicalExtent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BlueStoreBlobIdentity {
    Local(u32),
    Spanning(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlueStoreBlob {
    pub identity: BlueStoreBlobIdentity,
    pub owner: Option<Arc<BlueStoreObjectId>>,
    pub physical_extents: Vec<BlueStorePhysicalExtent>,
    pub on_disk_length: u32,
    pub logical_length: u32,
    pub compressed_length: Option<u32>,
    pub flags: BlueStoreBlobFlags,
    pub checksum: Option<BlueStoreChecksum>,
    pub checksum_words: Vec<u64>,
    pub unused_bitmap: Option<u16>,
    pub shared_blob_id: Option<u64>,
    pub use_tracker: Option<BlueStoreBlobUseTracker>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlueStorePhysicalExtent {
    pub offset: Option<u64>,
    pub length: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlueStoreBlobFlags {
    pub raw: u32,
    pub legacy_mutable: bool,
    pub compressed: bool,
    pub checksum: bool,
    pub has_unused: bool,
    pub shared: bool,
    pub unknown_bits: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlueStoreChecksumType {
    XxHash32,
    XxHash64,
    Crc32c,
    Crc32c16,
    Crc32c8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlueStoreChecksum {
    pub checksum_type: BlueStoreChecksumType,
    pub chunk_order: u8,
    pub encoded_length: usize,
    pub data_ceph_crc32c: u32,
    pub data_sha256: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlueStoreBlobUseTracker {
    V1LegacyRefMap {
        entries: Vec<BlueStoreBlobUseRef>,
    },
    V2 {
        allocation_unit_size: u32,
        declared_allocation_units: u32,
        referenced_bytes: Vec<u32>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlueStoreBlobUseRef {
    pub offset: u64,
    pub length: u32,
    pub refs: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlueStoreExtentFlags {
    pub raw: u8,
    pub contiguous: bool,
    pub zero_blob_offset: bool,
    pub same_length: bool,
    pub spanning: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlueStoreLogicalExtent {
    pub record_index: u32,
    pub logical_offset: u32,
    pub blob_offset: u32,
    pub length: u32,
    pub blob: BlueStoreBlobIdentity,
    pub defines_blob: bool,
    pub flags: BlueStoreExtentFlags,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlueStoreDeferred {
    pub reason: BlueStoreDeferredReason,
    pub encoded_length: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlueStoreDeferredReason {
    UnknownSuperField,
    SpanningBlobContextRequired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlueStoreSharedBlobRecord {
    pub sbid: u64,
    pub denc_version: u8,
    pub extent_refs: Vec<BlueStoreSharedBlobExtentRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlueStoreSharedBlobExtentRef {
    pub offset: u64,
    pub length: u32,
    pub refs: u32,
}
