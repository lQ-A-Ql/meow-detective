use persistence_sqlite::repositories::ceph_bluestore_semantic_repo::CephBluestoreObjectInventoryEntry;
use sha2::{Digest, Sha256};

pub(super) fn object_record_sha256(
    locator: &str,
    rule: &str,
    candidate_mask: u8,
    object: &CephBluestoreObjectInventoryEntry,
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"meow-detective/cephfs-metadata-object/v1\0");
    update_bytes(&mut digest, locator.as_bytes());
    update_bytes(&mut digest, rule.as_bytes());
    digest.update([candidate_mask]);
    update_bytes(&mut digest, object.object_identity_sha256.as_bytes());
    update_optional_bytes(&mut digest, object.object_key.as_deref());
    update_bytes(&mut digest, object.snap_hex.as_bytes());
    update_bytes(&mut digest, object.generation_hex.as_bytes());
    digest.update(object.size.to_be_bytes());
    digest.update(object.attribute_count.to_be_bytes());
    update_bytes(&mut digest, object.attributes_sha256.as_bytes());
    update_bytes(&mut digest, object.decode_status.as_bytes());
    update_optional_bytes(
        &mut digest,
        object.deferred_reason.as_deref().map(str::as_bytes),
    );
    hex::encode(digest.finalize())
}

pub(super) fn merged_inventory_sha256<'a>(
    descriptor_identity: &str,
    records: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"meow-detective/cephfs-merged-metadata-inventory/v1\0");
    update_bytes(&mut digest, descriptor_identity.as_bytes());
    for (locator, record_sha256) in records {
        update_bytes(&mut digest, locator.as_bytes());
        update_bytes(&mut digest, record_sha256.as_bytes());
    }
    hex::encode(digest.finalize())
}

fn update_optional_bytes(digest: &mut Sha256, value: Option<&[u8]>) {
    match value {
        Some(value) => {
            digest.update([1]);
            update_bytes(digest, value);
        }
        None => digest.update([0]),
    }
}

fn update_bytes(digest: &mut Sha256, value: &[u8]) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value);
}
