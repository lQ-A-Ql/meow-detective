use sha2::{Digest, Sha256};

use super::super::CephBluestoreObjectRecord;

pub fn canonical_collection_identity(
    kind: &str,
    pool: Option<u64>,
    seed: Option<u32>,
    shard: Option<u8>,
) -> Option<String> {
    match (kind, pool, seed, shard) {
        ("meta", None, None, None) => Some("meta".to_string()),
        ("head" | "temp", Some(pool), Some(seed), shard) => {
            let shard = shard.map_or_else(|| "--".to_string(), |value| format!("{value:02x}"));
            Some(format!("pg/{pool:016x}/{seed:08x}/{shard}/{kind}"))
        }
        _ => None,
    }
}

pub fn object_identity_sha256(record: &CephBluestoreObjectRecord) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"meow-detective/ceph-bluestore-object/v1\0");
    hasher.update(record.decoded_shard.to_be_bytes());
    hasher.update(record.decoded_pool.to_be_bytes());
    hasher.update(record.decoded_hash.to_be_bytes());
    hasher.update(record.decoded_bitwise_hash.to_be_bytes());
    update_bytes(&mut hasher, &record.object_namespace);
    match &record.object_key {
        Some(value) => {
            hasher.update([1]);
            update_bytes(&mut hasher, value);
        }
        None => hasher.update([0]),
    }
    update_bytes(&mut hasher, &record.object_name);
    hasher.update(record.snap_hex.as_bytes());
    hasher.update(record.generation_hex.as_bytes());
    hex::encode(hasher.finalize())
}

fn update_bytes(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}
