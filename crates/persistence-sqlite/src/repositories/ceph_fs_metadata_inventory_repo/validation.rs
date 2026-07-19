use std::collections::HashSet;

use super::{
    cephfs_metadata_inventory_sha256, CephFsMetadataInventory, CephFsMetadataInventoryRepoError,
    CephFsMetadataInventoryRepoResult, CEPHFS_METADATA_CLASSIFIER_PROFILE,
    CEPHFS_METADATA_SCHEMA_VERSION,
};

pub(super) fn validate_inventory(
    inventory: &CephFsMetadataInventory,
) -> CephFsMetadataInventoryRepoResult<()> {
    let manifest = &inventory.manifest;
    if !valid_identity(&manifest.filesystem_identity)
        || !valid_identity(&manifest.inventory_id)
        || !valid_identity(&manifest.data_source_id)
    {
        return invalid("manifest identity is empty or contains NUL");
    }
    if manifest.filesystem_id < 0 || manifest.metadata_pool_id < 0 {
        return invalid("filesystem or metadata pool identity is negative");
    }
    if manifest.schema_version != CEPHFS_METADATA_SCHEMA_VERSION
        || manifest.classifier_profile != CEPHFS_METADATA_CLASSIFIER_PROFILE
        || !manifest.complete
        || !valid_sha256(&manifest.source_semantic_sha256)
        || !valid_sha256(&manifest.inventory_sha256)
    {
        return invalid("manifest version, profile, completion, or digest is invalid");
    }
    if manifest.object_count != inventory.objects.len() as u64 {
        return invalid("manifest object count does not match projections");
    }
    let unknown_count = inventory
        .objects
        .iter()
        .filter(|object| object.classification_state == "metadata_only")
        .count() as u64;
    if manifest.unknown_object_count != unknown_count {
        return invalid("manifest unknown-object count does not match projections");
    }
    validate_objects(inventory)?;
    if cephfs_metadata_inventory_sha256(manifest, &inventory.objects) != manifest.inventory_sha256 {
        return invalid("manifest inventory digest does not match projections");
    }
    Ok(())
}

fn validate_objects(inventory: &CephFsMetadataInventory) -> CephFsMetadataInventoryRepoResult<()> {
    let mut identities = HashSet::new();
    let mut locators = HashSet::new();
    for object in &inventory.objects {
        if !identities.insert(object.object_identity_sha256.as_str())
            || !locators.insert(object.locator.as_str())
        {
            return invalid("object identity or locator is duplicated");
        }
        if !valid_sha256(&object.object_identity_sha256)
            || !valid_sha256(&object.record_sha256)
            || !valid_identity(&object.locator)
            || !valid_rule(&object.classifier_rule)
        {
            return invalid("object identity, locator, rule, or digest is invalid");
        }
        let state_matches_mask = match object.classification_state.as_str() {
            "candidate" => object.candidate_mask != 0,
            "classified" | "metadata_only" => object.candidate_mask == 0,
            _ => false,
        };
        if object.candidate_mask > 63 || !state_matches_mask {
            return invalid("object classification state does not match its candidate mask");
        }
    }
    Ok(())
}

fn valid_identity(value: &str) -> bool {
    !value.trim().is_empty() && !value.contains('\0')
}

fn valid_rule(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn invalid<T>(message: &'static str) -> CephFsMetadataInventoryRepoResult<T> {
    Err(CephFsMetadataInventoryRepoError::Invalid(message))
}
