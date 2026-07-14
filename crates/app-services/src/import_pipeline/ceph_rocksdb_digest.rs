use sha2::{Digest, Sha256};

use super::ceph_rocksdb_spool::{SpoolPointRef, SpoolProvenance, SpoolRange};

const DIGEST_SCHEMA_VERSION: u32 = 1;

pub(super) struct RecoveryDigests {
    point: CanonicalDigest,
    range: CanonicalDigest,
    latest: CanonicalDigest,
}

impl RecoveryDigests {
    pub(super) fn new(column_family_id: u32, column_family_name: &str) -> Self {
        let mut point = CanonicalDigest::new(b"meow.rocksdb.point-mutations");
        let mut range = CanonicalDigest::new(b"meow.rocksdb.range-tombstones");
        let mut latest = CanonicalDigest::new(b"meow.rocksdb.latest-state");
        for digest in [&mut point, &mut range, &mut latest] {
            digest.u32(DIGEST_SCHEMA_VERSION);
            digest.u32(column_family_id);
            digest.bytes(column_family_name.as_bytes());
        }
        Self {
            point,
            range,
            latest,
        }
    }

    pub(super) fn observe_point_ref(&mut self, point: SpoolPointRef<'_>) {
        self.point.bytes(point.user_key);
        self.point.u64(point.sequence);
        self.point.u8(point.value_type);
        self.point.bytes(point.value);
        self.point.provenance(point.provenance);
    }

    pub(super) fn observe_range(&mut self, range: &SpoolRange) {
        self.range.bytes(&range.start_key);
        self.range.bytes(&range.end_key);
        self.range.u64(range.sequence);
        self.range.provenance(range.provenance);
    }

    pub(super) fn observe_live(
        &mut self,
        user_key: &[u8],
        sequence: u64,
        origin: u8,
        value: &[u8],
    ) {
        self.latest.u8(1);
        self.latest.bytes(user_key);
        self.latest.u64(sequence);
        self.latest.u8(origin);
        self.latest.bytes(value);
    }

    pub(super) fn observe_deleted(&mut self, user_key: &[u8], sequence: u64, kind: u8) {
        self.latest.u8(0);
        self.latest.bytes(user_key);
        self.latest.u64(sequence);
        self.latest.u8(kind);
    }

    pub(super) fn finish(self) -> RecoveryDigestResult {
        RecoveryDigestResult {
            point_sha256: self.point.finish(),
            range_sha256: self.range.finish(),
            latest_state_sha256: self.latest.finish(),
        }
    }
}

pub(super) struct RecoveryDigestResult {
    pub(super) point_sha256: String,
    pub(super) range_sha256: String,
    pub(super) latest_state_sha256: String,
}

struct CanonicalDigest {
    hasher: Sha256,
}

impl CanonicalDigest {
    fn new(domain: &[u8]) -> Self {
        let mut digest = Self {
            hasher: Sha256::new(),
        };
        digest.bytes(domain);
        digest
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

    fn bytes(&mut self, value: &[u8]) {
        self.u64(value.len() as u64);
        self.hasher.update(value);
    }

    fn provenance(&mut self, provenance: SpoolProvenance) {
        self.u8(provenance.source_kind.encoded());
        self.u64(provenance.file_number);
        match provenance.level {
            Some(level) => {
                self.u8(1);
                self.u32(level);
            }
            None => self.u8(0),
        }
        self.u64(provenance.physical_offset);
        self.u64(provenance.primary_ordinal);
        self.u64(provenance.secondary_ordinal);
    }

    fn finish(self) -> String {
        hex::encode(self.hasher.finalize())
    }
}

pub(super) fn sharding_sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
#[path = "../../tests/unit/import_pipeline/ceph_rocksdb_digest.rs"]
mod tests;
