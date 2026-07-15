use sha2::{Digest, Sha256};

use super::{
    CephBluestoreOmapAggregate, CephBluestoreOmapScopeRecord, CephBluestoreRbdDirectoryRecord,
    CephBluestoreRbdHeaderRecord,
};

pub fn omap_aggregate_sha256(aggregate: &CephBluestoreOmapAggregate) -> String {
    let mut digest = CanonicalDigest::new();
    write_scan(&mut digest, aggregate);

    let mut scopes = aggregate.scopes.iter().collect::<Vec<_>>();
    scopes.sort_unstable_by(|left, right| left.scope_identity.cmp(&right.scope_identity));
    digest.u64(scopes.len() as u64);
    for record in scopes {
        write_scope(&mut digest, record);
    }

    let mut mappings = aggregate.directory_mappings.iter().collect::<Vec<_>>();
    mappings.sort_unstable_by(|left, right| {
        (
            left.scope_identity.as_str(),
            left.image_name.as_str(),
            left.image_id.as_str(),
        )
            .cmp(&(
                right.scope_identity.as_str(),
                right.image_name.as_str(),
                right.image_id.as_str(),
            ))
    });
    digest.u64(mappings.len() as u64);
    for record in mappings {
        write_directory_mapping(&mut digest, record);
    }

    let mut headers = aggregate.rbd_headers.iter().collect::<Vec<_>>();
    headers.sort_unstable_by(|left, right| left.image_id.cmp(&right.image_id));
    digest.u64(headers.len() as u64);
    for record in headers {
        write_header(&mut digest, record);
    }
    digest.finish()
}

fn write_scan(digest: &mut CanonicalDigest, aggregate: &CephBluestoreOmapAggregate) {
    let record = &aggregate.scan;
    digest.text("scan");
    digest.text(&record.inventory_id);
    digest.text(&record.data_source_id);
    digest.u32(record.schema_version);
    digest.text(&record.decode_profile);
    digest.text(&record.sharding_sha256);
    digest.text(&record.latest_state_sha256);
    digest.text(&record.semantic_sha256);
    digest.u64(record.scope_count);
    digest.u64(record.directory_mapping_count);
    digest.u64(record.rbd_header_count);
    digest.bool(record.profile_complete);
}

fn write_scope(digest: &mut CanonicalDigest, record: &CephBluestoreOmapScopeRecord) {
    digest.text("scope");
    digest.text(&record.inventory_id);
    digest.text(&record.scope_identity);
    digest.text(&record.key_family);
    digest.text(&record.pool_kind);
    digest.option_i64(record.pool_value_i64);
    digest.option_text(record.pool_value_hex.as_deref());
    digest.option_u32(record.hash);
    digest.text(&record.nid_hex);
    digest.option_text(record.owner_nid_hex.as_deref());
    digest.option_text(record.owner_family.as_deref());
    digest.option_text(record.owner_kind.as_deref());
    digest.option_text(record.owner_image_id.as_deref());
    digest.u64(record.entry_count);
    digest.u64(record.recognized_entry_count);
}

fn write_directory_mapping(digest: &mut CanonicalDigest, record: &CephBluestoreRbdDirectoryRecord) {
    digest.text("directory");
    digest.text(&record.inventory_id);
    digest.text(&record.scope_identity);
    digest.text(&record.owner_nid_hex);
    digest.text(&record.image_name);
    digest.text(&record.image_id);
    digest.bool(record.bidirectional);
}

fn write_header(digest: &mut CanonicalDigest, record: &CephBluestoreRbdHeaderRecord) {
    digest.text("header");
    digest.text(&record.inventory_id);
    digest.text(&record.scope_identity);
    digest.text(&record.owner_nid_hex);
    digest.text(&record.image_id);
    digest.option_text(record.size_hex.as_deref());
    digest.option_u8(record.object_order);
    digest.option_text(record.features_hex.as_deref());
    digest.option_text(record.object_prefix.as_deref());
    digest.option_text(record.stripe_unit_hex.as_deref());
    digest.option_text(record.stripe_count_hex.as_deref());
    digest.option_i64(record.data_pool_id);
}

struct CanonicalDigest {
    hasher: Sha256,
}

impl CanonicalDigest {
    fn new() -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"meow-detective/ceph-bluestore-omap/v1\0");
        Self { hasher }
    }

    fn finish(self) -> String {
        hex::encode(self.hasher.finalize())
    }

    fn text(&mut self, value: &str) {
        self.u64(value.len() as u64);
        self.hasher.update(value.as_bytes());
    }

    fn bool(&mut self, value: bool) {
        self.hasher.update([u8::from(value)]);
    }

    fn u8(&mut self, value: u8) {
        self.hasher.update([value]);
    }

    fn u32(&mut self, value: u32) {
        self.hasher.update(value.to_be_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.hasher.update(value.to_be_bytes());
    }

    fn i64(&mut self, value: i64) {
        self.hasher.update(value.to_be_bytes());
    }

    fn option_u8(&mut self, value: Option<u8>) {
        self.option_with(value, Self::u8);
    }

    fn option_u32(&mut self, value: Option<u32>) {
        self.option_with(value, Self::u32);
    }

    fn option_i64(&mut self, value: Option<i64>) {
        self.option_with(value, Self::i64);
    }

    fn option_text(&mut self, value: Option<&str>) {
        match value {
            Some(value) => {
                self.bool(true);
                self.text(value);
            }
            None => self.bool(false),
        }
    }

    fn option_with<T>(&mut self, value: Option<T>, write: fn(&mut Self, T)) {
        match value {
            Some(value) => {
                self.bool(true);
                write(self, value);
            }
            None => self.bool(false),
        }
    }
}
