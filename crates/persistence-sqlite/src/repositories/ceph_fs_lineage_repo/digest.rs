use sha2::{Digest, Sha256};

use super::CephFsDerivedLineageAggregate;

pub fn cephfs_lineage_fingerprint(aggregate: &CephFsDerivedLineageAggregate) -> String {
    let mut hasher = Sha256::new();
    update(&mut hasher, b"meow-detective-cephfs-lineage-v1");
    let lineage = &aggregate.lineage;
    for value in [
        lineage.derived_data_source_id.as_bytes(),
        lineage.parent_cluster_id.as_bytes(),
        lineage.cluster_identity.as_bytes(),
        lineage.filesystem_identity.as_bytes(),
        lineage.filesystem_name.as_bytes(),
        lineage.descriptor_state.as_bytes(),
        lineage.namespace_input_sha256.as_bytes(),
        lineage.namespace_projection_sha256.as_bytes(),
        lineage.namespace_assembly_sha256.as_bytes(),
        lineage.source_capability.as_bytes(),
        lineage.decoder_profile.as_bytes(),
    ] {
        update(&mut hasher, value);
    }
    update(&mut hasher, &lineage.filesystem_id.to_le_bytes());
    update(&mut hasher, &lineage.fsmap_epoch.to_le_bytes());
    update(&mut hasher, &lineage.mdsmap_epoch.to_le_bytes());
    update(&mut hasher, &lineage.metadata_pool_id.to_le_bytes());
    update(&mut hasher, &lineage.expected_replica_count.to_le_bytes());
    update(&mut hasher, &lineage.namespace_schema_version.to_le_bytes());
    update_optional(&mut hasher, lineage.journal_boundary_sha256.as_deref());
    for pool in &aggregate.pools {
        update(&mut hasher, &pool.pool_id.to_le_bytes());
        update(&mut hasher, pool.role.as_bytes());
        update(&mut hasher, &pool.ordinal.to_le_bytes());
        for source in &pool.sources {
            update(&mut hasher, &source.ordinal.to_le_bytes());
            update(&mut hasher, source.source_data_source_id.as_bytes());
            update(&mut hasher, source.inventory_id.as_bytes());
        }
    }
    for item in &aggregate.map_provenance {
        update(&mut hasher, &item.ordinal.to_le_bytes());
        update(&mut hasher, item.source_data_source_id.as_bytes());
        update(&mut hasher, item.inventory_id.as_bytes());
        update(&mut hasher, item.captured_at.as_bytes());
        update(&mut hasher, item.raw_fsmap_sha256.as_bytes());
        update(&mut hasher, item.raw_mdsmap_sha256.as_bytes());
    }
    hex::encode(hasher.finalize())
}

fn update(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}

fn update_optional(hasher: &mut Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            update(hasher, b"some");
            update(hasher, value.as_bytes());
        }
        None => update(hasher, b"none"),
    }
}
