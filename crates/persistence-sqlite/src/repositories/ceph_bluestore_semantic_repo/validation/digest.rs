mod checksum;

use sha2::{Digest, Sha256};

use super::super::{
    CephBluestoreBlobRecord, CephBluestoreChecksumChunkRecord, CephBluestoreCollectionRecord,
    CephBluestoreLogicalExtentRecord, CephBluestoreObjectRecord, CephBluestoreOnodeShardRecord,
    CephBluestorePhysicalExtentRecord, CephBluestoreSemanticAggregate,
    CephBluestoreSemanticScanRecord, CephBluestoreSharedBlobRecord,
    CephBluestoreSharedBlobRefRecord, CephBluestoreSuperRecord,
};
use crate::repositories::ceph_rocksdb_latest_state_repo::CephRocksdbLatestStateRecord;

pub fn latest_state_set_sha256(records: &[CephRocksdbLatestStateRecord]) -> String {
    latest_state_set_sha256_from_iter(records.iter().map(|record| {
        (
            record.column_family_id,
            record.column_family_name.as_str(),
            record.latest_state_sha256.as_str(),
        )
    }))
}

pub(crate) fn latest_state_set_sha256_from_scalars(records: &[(u32, String, String)]) -> String {
    latest_state_set_sha256_from_iter(
        records
            .iter()
            .map(|(id, name, digest)| (*id, name.as_str(), digest.as_str())),
    )
}

fn latest_state_set_sha256_from_iter<'a, I>(records: I) -> String
where
    I: IntoIterator<Item = (u32, &'a str, &'a str)>,
{
    let mut digest = Sha256::new();
    digest.update(b"meow-detective/ceph-rocksdb-latest-state-set/v1\0");
    let mut canonical = records.into_iter().collect::<Vec<_>>();
    digest.update((canonical.len() as u64).to_be_bytes());
    canonical.sort_unstable();
    for (column_family_id, column_family_name, latest_state_sha256) in canonical {
        digest.update(column_family_id.to_be_bytes());
        update_bytes(&mut digest, column_family_name.as_bytes());
        update_bytes(&mut digest, latest_state_sha256.as_bytes());
    }
    hex::encode(digest.finalize())
}

pub fn semantic_aggregate_sha256(aggregate: &CephBluestoreSemanticAggregate) -> String {
    let mut digest = CanonicalDigest::new();
    write_scan(&mut digest, &aggregate.scan);
    write_super(&mut digest, &aggregate.super_record);
    digest.records("collections", &aggregate.collections, write_collection);
    digest.records("objects", &aggregate.objects, write_object);
    digest.records("onode_shards", &aggregate.onode_shards, write_onode_shard);
    digest.records("blobs", &aggregate.blobs, write_blob);
    digest.tag("checksum_chunks");
    digest.u64(aggregate.checksum_chunks.len() as u64);
    for record in &aggregate.checksum_chunks {
        write_checksum_chunk(
            &mut digest,
            &aggregate.scan.inventory_id,
            &aggregate.objects,
            record,
        );
    }
    digest.records(
        "logical_extents",
        &aggregate.logical_extents,
        write_logical_extent,
    );
    digest.records(
        "physical_extents",
        &aggregate.physical_extents,
        write_physical_extent,
    );
    digest.records("shared_blobs", &aggregate.shared_blobs, write_shared_blob);
    digest.records(
        "shared_blob_refs",
        &aggregate.shared_blob_refs,
        write_shared_blob_ref,
    );
    digest.finish()
}

fn write_scan(digest: &mut CanonicalDigest, record: &CephBluestoreSemanticScanRecord) {
    digest.tag("scan");
    digest.text(&record.inventory_id);
    digest.u32(record.schema_version);
    digest.text(&record.decode_profile);
    digest.text(&record.sharding_sha256);
    digest.text(&record.latest_state_sha256);
    for count in [
        record.s_latest_count,
        record.s_decoded_count,
        record.s_deferred_count,
        record.c_latest_count,
        record.c_decoded_count,
        record.c_deferred_count,
        record.o_latest_count,
        record.o_decoded_count,
        record.o_deferred_count,
        record.x_latest_count,
        record.x_decoded_count,
        record.x_deferred_count,
        record.collection_count,
        record.object_count,
        record.blob_count,
        record.onode_shard_count,
        record.logical_extent_count,
        record.physical_extent_count,
        record.checksum_chunk_count,
        record.shared_blob_count,
        record.shared_ref_extent_count,
    ] {
        digest.u64(count);
    }
    digest.bool(record.profile_complete);
}

fn write_super(digest: &mut CanonicalDigest, record: &CephBluestoreSuperRecord) {
    digest.tag("super");
    digest.text(&record.inventory_id);
    digest.option_u64(record.nid_max);
    digest.option_u64(record.blobid_max);
    digest.option_u64(record.min_alloc_size);
    digest.option_i32(record.ondisk_format);
    digest.option_i32(record.min_compat_ondisk_format);
    digest.option_text(record.per_pool_omap.as_deref());
    digest.option_text(record.freelist_type.as_deref());
    digest.u64(record.observed_count);
    digest.u64(record.deferred_count);
}

fn write_collection(digest: &mut CanonicalDigest, record: &CephBluestoreCollectionRecord) {
    digest.tag("collection");
    digest.text(&record.inventory_id);
    digest.text(&record.collection_identity);
    digest.text(&record.kind);
    digest.option_u64(record.pool);
    digest.option_u32(record.seed);
    digest.option_u8(record.shard);
    digest.option_u32(record.bits);
    digest.option_u8(record.denc_version);
    digest.text(&record.decode_status);
    digest.option_text(record.deferred_reason.as_deref());
}

fn write_object(digest: &mut CanonicalDigest, record: &CephBluestoreObjectRecord) {
    digest.tag("object");
    digest.text(&record.inventory_id);
    digest.text(&record.object_identity_sha256);
    digest.i8(record.decoded_shard);
    digest.i64(record.decoded_pool);
    digest.u32(record.decoded_hash);
    digest.u32(record.decoded_bitwise_hash);
    digest.bytes(&record.object_namespace);
    digest.option_bytes(record.object_key.as_deref());
    digest.bytes(&record.object_name);
    digest.text(&record.snap_hex);
    digest.text(&record.generation_hex);
    digest.u8(record.onode_denc_version);
    digest.u64(record.nid);
    digest.u64(record.size);
    digest.u8(record.flags_raw);
    digest.bool(record.flag_omap);
    digest.bool(record.flag_pgmeta_omap);
    digest.bool(record.flag_per_pool_omap);
    digest.bool(record.flag_per_pg_omap);
    digest.u8(record.flags_unknown_bits);
    digest.u64(record.attribute_count);
    digest.u64(record.attribute_value_bytes);
    digest.text(&record.attributes_sha256);
    digest.u64(record.expected_object_size);
    digest.u64(record.expected_write_size);
    digest.u32(record.allocation_hint_flags);
    digest.u64(record.zone_ref_count);
    digest.text(&record.extent_storage);
    digest.u8(record.spanning_blob_version);
    digest.u64(record.declared_spanning_blob_count);
    digest.text(&record.decode_status);
    digest.option_text(record.deferred_reason.as_deref());
    digest.u64(record.onode_shard_count);
    digest.u64(record.blob_count);
    digest.u64(record.logical_extent_count);
    digest.u64(record.physical_extent_count);
}

fn write_onode_shard(digest: &mut CanonicalDigest, record: &CephBluestoreOnodeShardRecord) {
    digest.tag("onode_shard");
    digest.text(&record.inventory_id);
    digest.text(&record.object_identity_sha256);
    digest.u32(record.shard_ordinal);
    digest.u32(record.shard_offset);
    digest.u32(record.descriptor_bytes);
    digest.option_u8(record.payload_version);
    digest.option_u64(record.declared_extent_count);
    digest.option_u64(record.payload_encoded_length);
    digest.text(&record.decode_status);
    digest.option_text(record.deferred_reason.as_deref());
    digest.u64(record.logical_extent_count);
}

fn write_blob(digest: &mut CanonicalDigest, record: &CephBluestoreBlobRecord) {
    digest.tag("blob");
    digest.text(&record.inventory_id);
    digest.text(&record.object_identity_sha256);
    digest.u32(record.blob_ordinal);
    digest.text(&record.blob_kind);
    digest.text(&record.blob_id_hex);
    digest.option_text(record.shared_blob_id_hex.as_deref());
    digest.u64(record.logical_length);
    digest.u64(record.on_disk_length);
    digest.option_u64(record.compressed_length);
    digest.u32(record.flags_raw);
    digest.bool(record.flag_legacy_mutable);
    digest.bool(record.flag_compressed);
    digest.bool(record.flag_checksum);
    digest.bool(record.flag_has_unused);
    digest.bool(record.flag_shared);
    digest.u32(record.flags_unknown_bits);
    digest.option_u16(record.unused_bitmap);
    digest.option_text(record.checksum_type.as_deref());
    digest.option_u8(record.checksum_order);
    digest.option_u64(record.checksum_chunk_size);
    digest.option_u64(record.checksum_encoded_length);
    digest.u64(record.checksum_value_count);
    digest.option_u32(record.checksum_data_crc32c);
    digest.option_text(record.checksum_digest_sha256.as_deref());
    digest.option_text(record.use_tracker_kind.as_deref());
    digest.option_u64(record.use_tracker_allocation_unit_size);
    digest.option_u64(record.use_tracker_declared_allocation_units);
    digest.u64(record.use_tracker_entry_count);
    digest.option_text(record.use_tracker_sha256.as_deref());
    digest.u64(record.logical_extent_count);
    digest.u64(record.physical_extent_count);
}

fn write_logical_extent(digest: &mut CanonicalDigest, record: &CephBluestoreLogicalExtentRecord) {
    digest.tag("logical_extent");
    digest.text(&record.inventory_id);
    digest.text(&record.object_identity_sha256);
    digest.u32(record.extent_ordinal);
    digest.u64(record.logical_offset);
    digest.u64(record.length);
    digest.u32(record.blob_ordinal);
    digest.u64(record.blob_offset);
    digest.option_u32(record.shard_ordinal);
    digest.bool(record.defines_blob);
    digest.u8(record.flags_raw);
    digest.bool(record.flag_contiguous);
    digest.bool(record.flag_zero_blob_offset);
    digest.bool(record.flag_same_length);
    digest.bool(record.flag_spanning);
}

fn write_checksum_chunk(
    digest: &mut CanonicalDigest,
    inventory_id: &str,
    objects: &[CephBluestoreObjectRecord],
    record: &CephBluestoreChecksumChunkRecord,
) {
    match objects.get(record.object_ordinal as usize) {
        Some(object) => checksum::write_checksum_chunk(
            digest,
            inventory_id,
            &object.object_identity_sha256,
            record,
        ),
        None => {
            digest.tag("checksum_chunk");
            digest.text(inventory_id);
            digest.u32(record.object_ordinal);
            digest.text("");
            digest.u32(record.blob_ordinal);
            digest.u32(record.checksum_ordinal);
            digest.u64(record.chunk_offset);
            digest.u64(record.chunk_length);
            digest.checksum_hex(record.checksum_value, record.checksum_value_bytes);
        }
    }
}

fn write_physical_extent(digest: &mut CanonicalDigest, record: &CephBluestorePhysicalExtentRecord) {
    digest.tag("physical_extent");
    digest.text(&record.inventory_id);
    digest.text(&record.object_identity_sha256);
    digest.u32(record.blob_ordinal);
    digest.u32(record.extent_ordinal);
    digest.u64(record.blob_offset);
    digest.u8(record.device_id);
    digest.option_text(record.physical_offset_hex.as_deref());
    digest.u64(record.length);
}

fn write_shared_blob(digest: &mut CanonicalDigest, record: &CephBluestoreSharedBlobRecord) {
    digest.tag("shared_blob");
    digest.text(&record.inventory_id);
    digest.text(&record.shared_blob_id_hex);
    digest.option_u8(record.denc_version);
    digest.text(&record.decode_status);
    digest.option_text(record.deferred_reason.as_deref());
    digest.u64(record.ref_extent_count);
    digest.u64(record.total_ref_bytes);
    digest.u64(record.total_refs);
}

fn write_shared_blob_ref(digest: &mut CanonicalDigest, record: &CephBluestoreSharedBlobRefRecord) {
    digest.tag("shared_blob_ref");
    digest.text(&record.inventory_id);
    digest.text(&record.shared_blob_id_hex);
    digest.u32(record.ref_ordinal);
    digest.text(&record.ref_offset_hex);
    digest.u64(record.length);
    digest.u64(record.refs);
}

fn update_bytes(digest: &mut Sha256, value: &[u8]) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value);
}

struct CanonicalDigest {
    hasher: Sha256,
}

impl CanonicalDigest {
    fn new() -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"meow-detective/ceph-bluestore-semantic/v1\0");
        Self { hasher }
    }

    fn finish(self) -> String {
        hex::encode(self.hasher.finalize())
    }

    fn records<T>(&mut self, tag: &str, records: &[T], write: fn(&mut Self, &T)) {
        self.tag(tag);
        self.u64(records.len() as u64);
        for record in records {
            write(self, record);
        }
    }

    fn tag(&mut self, value: &str) {
        self.text(value);
    }

    fn text(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn bytes(&mut self, value: &[u8]) {
        self.u64(value.len() as u64);
        self.hasher.update(value);
    }

    fn checksum_hex(&mut self, value: u64, width_bytes: u8) {
        if !(1..=8).contains(&width_bytes) {
            self.u8(width_bytes);
            self.u64(value);
            return;
        }
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let length = usize::from(width_bytes) * 2;
        let mut encoded = [0u8; 16];
        for (index, output) in encoded[..length].iter_mut().enumerate() {
            let shift = (length - index - 1) * 4;
            *output = HEX[((value >> shift) & 0x0f) as usize];
        }
        self.bytes(&encoded[..length]);
    }

    fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }

    fn u8(&mut self, value: u8) {
        self.hasher.update([value]);
    }

    fn i8(&mut self, value: i8) {
        self.hasher.update(value.to_be_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.hasher.update(value.to_be_bytes());
    }

    fn u16(&mut self, value: u16) {
        self.hasher.update(value.to_be_bytes());
    }

    fn i32(&mut self, value: i32) {
        self.hasher.update(value.to_be_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.hasher.update(value.to_be_bytes());
    }

    fn i64(&mut self, value: i64) {
        self.hasher.update(value.to_be_bytes());
    }

    fn option_bytes(&mut self, value: Option<&[u8]>) {
        match value {
            Some(value) => {
                self.u8(1);
                self.bytes(value);
            }
            None => self.u8(0),
        }
    }

    fn option_text(&mut self, value: Option<&str>) {
        match value {
            Some(value) => {
                self.u8(1);
                self.text(value);
            }
            None => self.u8(0),
        }
    }

    fn option_u8(&mut self, value: Option<u8>) {
        self.option_with(value, Self::u8);
    }

    fn option_u32(&mut self, value: Option<u32>) {
        self.option_with(value, Self::u32);
    }

    fn option_u16(&mut self, value: Option<u16>) {
        self.option_with(value, Self::u16);
    }

    fn option_i32(&mut self, value: Option<i32>) {
        self.option_with(value, Self::i32);
    }

    fn option_u64(&mut self, value: Option<u64>) {
        self.option_with(value, Self::u64);
    }

    fn option_with<T>(&mut self, value: Option<T>, write: fn(&mut Self, T)) {
        match value {
            Some(value) => {
                self.u8(1);
                write(self, value);
            }
            None => self.u8(0),
        }
    }
}
