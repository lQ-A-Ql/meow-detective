use std::collections::BTreeMap;

use ceph_wire::{
    decode_bluestore_latest_value, BlueStoreCollectionId, BlueStoreDecodedRecord,
    BlueStoreKeySpace, BlueStoreOmapMode, BlueStoreSemanticLimits, BlueStoreSharedBlobRecord,
    BlueStoreSuperRecord,
};
use persistence_sqlite::repositories::{
    ceph_bluestore_semantic_repo::{
        canonical_collection_identity, latest_state_set_sha256, semantic_aggregate_sha256,
        CephBluestoreCollectionRecord, CephBluestoreSemanticAggregate,
        CephBluestoreSemanticScanRecord, CephBluestoreSharedBlobRecord,
        CephBluestoreSharedBlobRefRecord, CephBluestoreSuperRecord,
        BLUESTORE_SEMANTIC_DECODE_PROFILE, BLUESTORE_SEMANTIC_SCHEMA_VERSION,
    },
    ceph_rocksdb_latest_state_repo::CephRocksdbLatestStateRecord,
};
use transport::CommandError;

use super::{
    object::{finalize_objects, observe_object, PendingObject},
    routing::route_bluestore_key,
};
use crate::import_pipeline::ceph_rocksdb_sharding::RocksdbShardingDefinition;

const MAX_SEMANTIC_LIVE_VALUES: u64 = 5_000_000;
const MAX_SEMANTIC_RETAINED_INPUT_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Default)]
pub(in crate::import_pipeline) struct BlueStoreSemanticFragment {
    super_record: SuperAccumulator,
    collections: BTreeMap<String, CephBluestoreCollectionRecord>,
    objects: BTreeMap<ceph_wire::BlueStoreObjectId, PendingObject>,
    shared_blobs: BTreeMap<u64, SharedBlobRows>,
    latest_counts: [u64; 4],
    retained_input_bytes: u64,
}

impl BlueStoreSemanticFragment {
    pub(in crate::import_pipeline) fn observe_latest_value(
        &mut self,
        sharding: &RocksdbShardingDefinition,
        physical_column_family: &str,
        user_key: &[u8],
        value: &[u8],
    ) -> Result<(), CommandError> {
        let Some(routed) = route_bluestore_key(sharding, physical_column_family, user_key)? else {
            return Ok(());
        };
        self.claim_input(user_key.len(), value.len())?;
        increment(&mut self.latest_counts[key_space_index(routed.key_space)])?;
        if routed.key_space == BlueStoreKeySpace::Object {
            return observe_object(
                &mut self.objects,
                routed.logical_key,
                value,
                BlueStoreSemanticLimits::default(),
            );
        }
        let decoded = decode_bluestore_latest_value(
            routed.key_space,
            routed.logical_key,
            value,
            BlueStoreSemanticLimits::default(),
        )
        .map_err(map_decode_error)?;
        self.observe_decoded(decoded)
    }

    pub(in crate::import_pipeline) fn merge(&mut self, fragment: Self) -> Result<(), CommandError> {
        self.super_record.merge(fragment.super_record)?;
        merge_unique(&mut self.collections, fragment.collections, "collection")?;
        merge_unique(&mut self.objects, fragment.objects, "object")?;
        merge_unique(&mut self.shared_blobs, fragment.shared_blobs, "shared blob")?;
        for (target, source) in self.latest_counts.iter_mut().zip(fragment.latest_counts) {
            *target = target
                .checked_add(source)
                .ok_or_else(|| semantic_error("latest-state count overflow"))?;
        }
        self.retained_input_bytes = self
            .retained_input_bytes
            .checked_add(fragment.retained_input_bytes)
            .filter(|value| *value <= MAX_SEMANTIC_RETAINED_INPUT_BYTES)
            .ok_or_else(|| {
                CommandError::unsupported(
                    "BlueStore semantic input exceeds the bounded resident-memory profile",
                )
            })?;
        Ok(())
    }

    pub(in crate::import_pipeline) fn finish(
        self,
        inventory_id: &str,
        sharding_sha256: &str,
        device_size: u64,
        latest_state: &[CephRocksdbLatestStateRecord],
    ) -> Result<CephBluestoreSemanticAggregate, CommandError> {
        let objects = finalize_objects(inventory_id, self.objects, device_size)?;
        let mut collections = self.collections.into_values().collect::<Vec<_>>();
        for record in &mut collections {
            record.inventory_id = inventory_id.to_string();
        }
        let mut shared = self.shared_blobs.into_values().collect::<Vec<_>>();
        for rows in &mut shared {
            rows.record.inventory_id = inventory_id.to_string();
            for record in &mut rows.refs {
                record.inventory_id = inventory_id.to_string();
            }
        }
        let shared_blobs = shared
            .iter()
            .map(|rows| rows.record.clone())
            .collect::<Vec<_>>();
        let shared_blob_refs = shared
            .into_iter()
            .flat_map(|rows| rows.refs)
            .collect::<Vec<_>>();
        let mut aggregate = CephBluestoreSemanticAggregate {
            scan: CephBluestoreSemanticScanRecord {
                inventory_id: inventory_id.to_string(),
                schema_version: BLUESTORE_SEMANTIC_SCHEMA_VERSION,
                decode_profile: BLUESTORE_SEMANTIC_DECODE_PROFILE.to_string(),
                sharding_sha256: sharding_sha256.to_string(),
                latest_state_sha256: latest_state_set_sha256(latest_state),
                semantic_sha256: String::new(),
                s_latest_count: self.latest_counts[0],
                s_decoded_count: self.latest_counts[0]
                    .checked_sub(self.super_record.deferred_count)
                    .ok_or_else(|| semantic_error("super deferred count exceeds latest count"))?,
                s_deferred_count: self.super_record.deferred_count,
                c_latest_count: self.latest_counts[1],
                c_decoded_count: self.latest_counts[1],
                c_deferred_count: 0,
                o_latest_count: self.latest_counts[2],
                o_decoded_count: self.latest_counts[2],
                o_deferred_count: 0,
                x_latest_count: self.latest_counts[3],
                x_decoded_count: self.latest_counts[3],
                x_deferred_count: 0,
                collection_count: collections.len() as u64,
                object_count: objects.objects.len() as u64,
                blob_count: objects.blobs.len() as u64,
                onode_shard_count: objects.onode_shards.len() as u64,
                logical_extent_count: objects.logical_extents.len() as u64,
                physical_extent_count: objects.physical_extents.len() as u64,
                checksum_chunk_count: objects.checksum_chunks.len() as u64,
                shared_blob_count: shared_blobs.len() as u64,
                shared_ref_extent_count: shared_blob_refs.len() as u64,
                profile_complete: true,
            },
            super_record: self.super_record.finish(inventory_id),
            collections,
            objects: objects.objects,
            onode_shards: objects.onode_shards,
            blobs: objects.blobs,
            checksum_chunks: objects.checksum_chunks,
            logical_extents: objects.logical_extents,
            physical_extents: objects.physical_extents,
            shared_blobs,
            shared_blob_refs,
        };
        aggregate.scan.semantic_sha256 = semantic_aggregate_sha256(&aggregate);
        Ok(aggregate)
    }

    fn observe_decoded(&mut self, decoded: BlueStoreDecodedRecord) -> Result<(), CommandError> {
        match decoded {
            BlueStoreDecodedRecord::Super(record) => self.super_record.observe(record),
            BlueStoreDecodedRecord::Collection(record) => self.observe_collection(record),
            BlueStoreDecodedRecord::SharedBlob(record) => self.observe_shared_blob(record),
            BlueStoreDecodedRecord::Object(_) => Err(semantic_error(
                "object decoder bypassed the contextual extent-shard path",
            )),
        }
    }

    fn observe_collection(
        &mut self,
        decoded: ceph_wire::BlueStoreCollectionRecord,
    ) -> Result<(), CommandError> {
        let (kind, pool, seed, shard) = match decoded.collection {
            BlueStoreCollectionId::Meta => ("meta", None, None, None),
            BlueStoreCollectionId::Pg {
                pool,
                seed,
                shard,
                kind,
            } => (
                match kind {
                    ceph_wire::BlueStoreCollectionKind::Head => "head",
                    ceph_wire::BlueStoreCollectionKind::Temp => "temp",
                },
                Some(pool),
                Some(seed),
                shard,
            ),
        };
        let identity = canonical_collection_identity(kind, pool, seed, shard)
            .ok_or_else(|| semantic_error("collection identity is not canonical"))?;
        let row = CephBluestoreCollectionRecord {
            inventory_id: String::new(),
            collection_identity: identity.clone(),
            kind: kind.to_string(),
            pool,
            seed,
            shard,
            bits: Some(decoded.cnode.bits),
            denc_version: Some(decoded.cnode.denc_version),
            decode_status: "parsed".to_string(),
            deferred_reason: None,
        };
        if self.collections.insert(identity, row).is_some() {
            return Err(semantic_error("duplicate collection identity"));
        }
        Ok(())
    }

    fn observe_shared_blob(
        &mut self,
        decoded: BlueStoreSharedBlobRecord,
    ) -> Result<(), CommandError> {
        let id = format!("{:016x}", decoded.sbid);
        let mut total_ref_bytes = 0u64;
        let mut total_refs = 0u64;
        let refs = decoded
            .extent_refs
            .iter()
            .enumerate()
            .map(|(ordinal, entry)| {
                total_ref_bytes = total_ref_bytes
                    .checked_add(u64::from(entry.length))
                    .ok_or_else(|| semantic_error("shared blob referenced-byte overflow"))?;
                total_refs = total_refs
                    .checked_add(u64::from(entry.refs))
                    .ok_or_else(|| semantic_error("shared blob reference-count overflow"))?;
                Ok(CephBluestoreSharedBlobRefRecord {
                    inventory_id: String::new(),
                    shared_blob_id_hex: id.clone(),
                    ref_ordinal: ordinal as u32,
                    ref_offset_hex: format!("{:016x}", entry.offset),
                    length: u64::from(entry.length),
                    refs: u64::from(entry.refs),
                })
            })
            .collect::<Result<Vec<_>, CommandError>>()?;
        let rows = SharedBlobRows {
            record: CephBluestoreSharedBlobRecord {
                inventory_id: String::new(),
                shared_blob_id_hex: id,
                denc_version: Some(decoded.denc_version),
                decode_status: "parsed".to_string(),
                deferred_reason: None,
                ref_extent_count: refs.len() as u64,
                total_ref_bytes,
                total_refs,
            },
            refs,
        };
        if self.shared_blobs.insert(decoded.sbid, rows).is_some() {
            return Err(semantic_error("duplicate shared blob identity"));
        }
        Ok(())
    }

    fn claim_input(&mut self, key_bytes: usize, value_bytes: usize) -> Result<(), CommandError> {
        let live_values = self
            .latest_counts
            .into_iter()
            .try_fold(0u64, u64::checked_add)
            .ok_or_else(|| semantic_error("semantic latest-value count overflow"))?;
        if live_values >= MAX_SEMANTIC_LIVE_VALUES {
            return Err(CommandError::unsupported(
                "BlueStore semantic latest-value count exceeds the supported profile",
            ));
        }
        let bytes = key_bytes
            .checked_add(value_bytes)
            .and_then(|value| u64::try_from(value).ok())
            .ok_or_else(|| semantic_error("semantic input byte count overflow"))?;
        self.retained_input_bytes = self
            .retained_input_bytes
            .checked_add(bytes)
            .filter(|value| *value <= MAX_SEMANTIC_RETAINED_INPUT_BYTES)
            .ok_or_else(|| {
                CommandError::unsupported(
                    "BlueStore semantic input exceeds the bounded resident-memory profile",
                )
            })?;
        Ok(())
    }
}

#[derive(Default)]
struct SuperAccumulator {
    nid_max: Option<u64>,
    blobid_max: Option<u64>,
    min_alloc_size: Option<u64>,
    ondisk_format: Option<i32>,
    min_compat_ondisk_format: Option<i32>,
    per_pool_omap: Option<String>,
    freelist_type: Option<String>,
    observed_count: u64,
    deferred_count: u64,
}

impl SuperAccumulator {
    fn observe(&mut self, record: BlueStoreSuperRecord) -> Result<(), CommandError> {
        increment(&mut self.observed_count)?;
        match record {
            BlueStoreSuperRecord::NidMax(value) => set_once(&mut self.nid_max, value, "nid_max"),
            BlueStoreSuperRecord::BlobIdMax(value) => {
                set_once(&mut self.blobid_max, value, "blobid_max")
            }
            BlueStoreSuperRecord::MinAllocSize(value) => {
                set_once(&mut self.min_alloc_size, value, "min_alloc_size")
            }
            BlueStoreSuperRecord::OndiskFormat(value) => {
                set_once(&mut self.ondisk_format, value, "ondisk_format")
            }
            BlueStoreSuperRecord::MinCompatOndiskFormat(value) => set_once(
                &mut self.min_compat_ondisk_format,
                value,
                "min_compat_ondisk_format",
            ),
            BlueStoreSuperRecord::PerPoolOmap(value) => set_once(
                &mut self.per_pool_omap,
                omap_mode(value).to_string(),
                "per_pool_omap",
            ),
            BlueStoreSuperRecord::FreelistType(value) => {
                set_once(&mut self.freelist_type, value, "freelist_type")
            }
            BlueStoreSuperRecord::Unknown { .. } => increment(&mut self.deferred_count),
        }
    }

    fn merge(&mut self, other: Self) -> Result<(), CommandError> {
        merge_option(&mut self.nid_max, other.nid_max, "nid_max")?;
        merge_option(&mut self.blobid_max, other.blobid_max, "blobid_max")?;
        merge_option(
            &mut self.min_alloc_size,
            other.min_alloc_size,
            "min_alloc_size",
        )?;
        merge_option(
            &mut self.ondisk_format,
            other.ondisk_format,
            "ondisk_format",
        )?;
        merge_option(
            &mut self.min_compat_ondisk_format,
            other.min_compat_ondisk_format,
            "min_compat_ondisk_format",
        )?;
        merge_option(
            &mut self.per_pool_omap,
            other.per_pool_omap,
            "per_pool_omap",
        )?;
        merge_option(
            &mut self.freelist_type,
            other.freelist_type,
            "freelist_type",
        )?;
        self.observed_count = self
            .observed_count
            .checked_add(other.observed_count)
            .ok_or_else(|| semantic_error("super observed-count overflow"))?;
        self.deferred_count = self
            .deferred_count
            .checked_add(other.deferred_count)
            .ok_or_else(|| semantic_error("super deferred-count overflow"))?;
        Ok(())
    }

    fn finish(self, inventory_id: &str) -> CephBluestoreSuperRecord {
        CephBluestoreSuperRecord {
            inventory_id: inventory_id.to_string(),
            nid_max: self.nid_max,
            blobid_max: self.blobid_max,
            min_alloc_size: self.min_alloc_size,
            ondisk_format: self.ondisk_format,
            min_compat_ondisk_format: self.min_compat_ondisk_format,
            per_pool_omap: self.per_pool_omap,
            freelist_type: self.freelist_type,
            observed_count: self.observed_count,
            deferred_count: self.deferred_count,
        }
    }
}

struct SharedBlobRows {
    record: CephBluestoreSharedBlobRecord,
    refs: Vec<CephBluestoreSharedBlobRefRecord>,
}

fn key_space_index(key_space: BlueStoreKeySpace) -> usize {
    match key_space {
        BlueStoreKeySpace::Super => 0,
        BlueStoreKeySpace::Collection => 1,
        BlueStoreKeySpace::Object => 2,
        BlueStoreKeySpace::SharedBlob => 3,
    }
}

fn omap_mode(value: BlueStoreOmapMode) -> &'static str {
    match value {
        BlueStoreOmapMode::Bulk => "bulk",
        BlueStoreOmapMode::PerPool => "perPool",
        BlueStoreOmapMode::PerPg => "perPg",
    }
}

fn set_once<T>(target: &mut Option<T>, value: T, field: &str) -> Result<(), CommandError> {
    if target.replace(value).is_some() {
        return Err(semantic_error(format!("duplicate super field {field}")));
    }
    Ok(())
}

fn merge_option<T>(
    target: &mut Option<T>,
    value: Option<T>,
    field: &str,
) -> Result<(), CommandError> {
    if let Some(value) = value {
        set_once(target, value, field)?;
    }
    Ok(())
}

fn merge_unique<K: Ord, V>(
    target: &mut BTreeMap<K, V>,
    source: BTreeMap<K, V>,
    kind: &str,
) -> Result<(), CommandError> {
    for (key, value) in source {
        if target.insert(key, value).is_some() {
            return Err(semantic_error(format!(
                "duplicate {kind} across column families"
            )));
        }
    }
    Ok(())
}

fn increment(value: &mut u64) -> Result<(), CommandError> {
    *value = value
        .checked_add(1)
        .ok_or_else(|| semantic_error("semantic count overflow"))?;
    Ok(())
}

fn map_decode_error(error: ceph_wire::CephWireError) -> CommandError {
    let message = format!("BlueStore semantic decode failed: {error}");
    if matches!(
        error,
        ceph_wire::CephWireError::LengthLimit { .. }
            | ceph_wire::CephWireError::UnsupportedBlueStoreDencVersion { .. }
            | ceph_wire::CephWireError::UnknownBlueStoreBlobFlags { .. }
            | ceph_wire::CephWireError::UnknownBlueStoreChecksumType { .. }
    ) {
        CommandError::unsupported(message)
    } else {
        CommandError::parser(message)
    }
}

fn semantic_error(message: impl Into<String>) -> CommandError {
    CommandError::parser(format!(
        "BlueStore semantic recovery failed: {}",
        message.into()
    ))
}
