use sha2::{Digest, Sha256};

use super::{CephFsMetadataInventoryManifest, CephFsMetadataObjectProjection};

pub fn cephfs_metadata_inventory_sha256(
    manifest: &CephFsMetadataInventoryManifest,
    objects: &[CephFsMetadataObjectProjection],
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"meow-detective/cephfs-metadata-inventory/v1\0");
    update_bytes(&mut digest, manifest.filesystem_identity.as_bytes());
    update_bytes(&mut digest, manifest.inventory_id.as_bytes());
    update_bytes(&mut digest, manifest.data_source_id.as_bytes());
    digest.update(manifest.filesystem_id.to_be_bytes());
    digest.update(manifest.fsmap_epoch.to_be_bytes());
    digest.update(manifest.metadata_pool_id.to_be_bytes());
    digest.update(manifest.schema_version.to_be_bytes());
    update_bytes(&mut digest, manifest.classifier_profile.as_bytes());
    update_bytes(&mut digest, manifest.source_semantic_sha256.as_bytes());
    digest.update(manifest.object_count.to_be_bytes());
    digest.update(manifest.unknown_object_count.to_be_bytes());
    digest.update([u8::from(manifest.complete)]);

    let mut canonical = objects.iter().collect::<Vec<_>>();
    canonical.sort_by(|left, right| {
        (
            &left.locator,
            &left.object_identity_sha256,
            &left.record_sha256,
        )
            .cmp(&(
                &right.locator,
                &right.object_identity_sha256,
                &right.record_sha256,
            ))
    });
    for object in canonical {
        update_bytes(&mut digest, object.object_identity_sha256.as_bytes());
        update_bytes(&mut digest, object.locator.as_bytes());
        digest.update([object.candidate_mask]);
        update_bytes(&mut digest, object.classification_state.as_bytes());
        update_bytes(&mut digest, object.classifier_rule.as_bytes());
        update_bytes(&mut digest, object.record_sha256.as_bytes());
    }
    hex::encode(digest.finalize())
}

fn update_bytes(digest: &mut Sha256, value: &[u8]) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value);
}
