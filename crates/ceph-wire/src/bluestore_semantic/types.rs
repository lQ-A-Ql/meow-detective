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
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
        extents: BlueStoreExtentStorage,
    },
    Deferred {
        spanning_blob_version: u8,
        declared_spanning_blob_count: u32,
        payload: BlueStoreDeferred,
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
    pub status: BlueStorePayloadStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlueStorePayloadStatus {
    Parsed,
    Deferred(BlueStoreDeferred),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlueStoreDeferred {
    pub reason: BlueStoreDeferredReason,
    pub encoded_length: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlueStoreDeferredReason {
    UnknownSuperField,
    SpanningBlobRecords,
    ExtentRecords,
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
