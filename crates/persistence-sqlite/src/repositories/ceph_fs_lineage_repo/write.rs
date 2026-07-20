use rusqlite::{params, Connection};

use crate::connection::DbResult;

use super::CephFsDerivedLineageAggregate;

pub(super) fn insert(conn: &Connection, aggregate: &CephFsDerivedLineageAggregate) -> DbResult<()> {
    let lineage = &aggregate.lineage;
    conn.execute(
        "INSERT INTO ceph_fs_derived_lineage (
            derived_data_source_id, parent_cluster_id, cluster_identity,
            filesystem_identity, filesystem_id, filesystem_name, fsmap_epoch, mdsmap_epoch,
            descriptor_state, metadata_pool_id, expected_replica_count,
            namespace_input_sha256, namespace_projection_sha256,
            namespace_assembly_sha256, source_capability, namespace_schema_version,
            decoder_profile, journal_boundary_sha256,
            lineage_fingerprint
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                   ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
        params![
            lineage.derived_data_source_id,
            lineage.parent_cluster_id,
            lineage.cluster_identity,
            lineage.filesystem_identity,
            lineage.filesystem_id,
            lineage.filesystem_name,
            lineage.fsmap_epoch,
            lineage.mdsmap_epoch,
            lineage.descriptor_state,
            lineage.metadata_pool_id,
            lineage.expected_replica_count,
            lineage.namespace_input_sha256,
            lineage.namespace_projection_sha256,
            lineage.namespace_assembly_sha256,
            lineage.source_capability,
            lineage.namespace_schema_version,
            lineage.decoder_profile,
            lineage.journal_boundary_sha256,
            lineage.lineage_fingerprint,
        ],
    )?;
    let mut pool_statement = conn.prepare_cached(
        "INSERT INTO ceph_fs_derived_pools (
            derived_data_source_id, pool_id, role, ordinal
         ) VALUES (?1, ?2, ?3, ?4)",
    )?;
    let mut source_statement = conn.prepare_cached(
        "INSERT INTO ceph_fs_derived_pool_sources (
            derived_data_source_id, pool_id, ordinal,
            source_data_source_id, inventory_id
         ) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for pool in &aggregate.pools {
        pool_statement.execute(params![
            lineage.derived_data_source_id,
            pool.pool_id,
            pool.role,
            pool.ordinal,
        ])?;
        for source in &pool.sources {
            source_statement.execute(params![
                lineage.derived_data_source_id,
                pool.pool_id,
                source.ordinal,
                source.source_data_source_id,
                source.inventory_id,
            ])?;
        }
    }
    let mut provenance_statement = conn.prepare_cached(
        "INSERT INTO ceph_fs_derived_map_provenance (
            derived_data_source_id, ordinal, source_data_source_id, inventory_id,
            captured_at, raw_fsmap_sha256, raw_mdsmap_sha256
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for item in &aggregate.map_provenance {
        provenance_statement.execute(params![
            lineage.derived_data_source_id,
            item.ordinal,
            item.source_data_source_id,
            item.inventory_id,
            item.captured_at,
            item.raw_fsmap_sha256,
            item.raw_mdsmap_sha256,
        ])?;
    }
    Ok(())
}
