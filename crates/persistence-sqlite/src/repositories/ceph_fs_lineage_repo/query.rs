use rusqlite::{params, Connection, OptionalExtension};

use crate::connection::DbResult;

use super::{
    CephFsDerivedLineageAggregate, CephFsDerivedLineageRecord, CephFsDerivedMapProvenanceRecord,
    CephFsDerivedPoolRecord, CephFsDerivedPoolSourceRecord,
};

pub(super) fn find(
    conn: &Connection,
    data_source_id: &str,
) -> DbResult<Option<CephFsDerivedLineageAggregate>> {
    let Some(lineage) = conn
        .query_row(
            "SELECT derived_data_source_id, parent_cluster_id, cluster_identity,
                    filesystem_identity, filesystem_id, filesystem_name, fsmap_epoch, mdsmap_epoch,
                    descriptor_state, metadata_pool_id, expected_replica_count,
                    namespace_input_sha256, namespace_projection_sha256,
                    namespace_assembly_sha256, source_capability, namespace_schema_version,
                    decoder_profile,
                    journal_boundary_sha256, lineage_fingerprint
             FROM ceph_fs_derived_lineage
             WHERE derived_data_source_id = ?1",
            [data_source_id],
            map_lineage,
        )
        .optional()?
    else {
        return Ok(None);
    };
    Ok(Some(CephFsDerivedLineageAggregate {
        pools: load_pools(conn, data_source_id)?,
        map_provenance: load_map_provenance(conn, data_source_id)?,
        lineage,
    }))
}

fn load_pools(conn: &Connection, data_source_id: &str) -> DbResult<Vec<CephFsDerivedPoolRecord>> {
    let mut statement = conn.prepare(
        "SELECT pool_id, role, ordinal
         FROM ceph_fs_derived_pools
         WHERE derived_data_source_id = ?1
         ORDER BY CASE role WHEN 'metadata' THEN 0 ELSE 1 END, ordinal",
    )?;
    let rows = statement.query_map([data_source_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            decode_u32(row.get(2)?, 2)?,
        ))
    })?;
    let mut pools = Vec::new();
    for row in rows {
        let (pool_id, role, ordinal) = row?;
        pools.push(CephFsDerivedPoolRecord {
            pool_id,
            role,
            ordinal,
            sources: load_pool_sources(conn, data_source_id, pool_id)?,
        });
    }
    Ok(pools)
}

fn load_pool_sources(
    conn: &Connection,
    data_source_id: &str,
    pool_id: i64,
) -> DbResult<Vec<CephFsDerivedPoolSourceRecord>> {
    let mut statement = conn.prepare(
        "SELECT ordinal, source_data_source_id, inventory_id
         FROM ceph_fs_derived_pool_sources
         WHERE derived_data_source_id = ?1 AND pool_id = ?2
         ORDER BY ordinal",
    )?;
    let rows = statement.query_map(params![data_source_id, pool_id], |row| {
        Ok(CephFsDerivedPoolSourceRecord {
            ordinal: decode_u32(row.get(0)?, 0)?,
            source_data_source_id: row.get(1)?,
            inventory_id: row.get(2)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn load_map_provenance(
    conn: &Connection,
    data_source_id: &str,
) -> DbResult<Vec<CephFsDerivedMapProvenanceRecord>> {
    let mut statement = conn.prepare(
        "SELECT ordinal, source_data_source_id, inventory_id, captured_at,
                raw_fsmap_sha256, raw_mdsmap_sha256
         FROM ceph_fs_derived_map_provenance
         WHERE derived_data_source_id = ?1
         ORDER BY ordinal",
    )?;
    let rows = statement.query_map([data_source_id], |row| {
        Ok(CephFsDerivedMapProvenanceRecord {
            ordinal: decode_u32(row.get(0)?, 0)?,
            source_data_source_id: row.get(1)?,
            inventory_id: row.get(2)?,
            captured_at: row.get(3)?,
            raw_fsmap_sha256: row.get(4)?,
            raw_mdsmap_sha256: row.get(5)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn map_lineage(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephFsDerivedLineageRecord> {
    Ok(CephFsDerivedLineageRecord {
        derived_data_source_id: row.get(0)?,
        parent_cluster_id: row.get(1)?,
        cluster_identity: row.get(2)?,
        filesystem_identity: row.get(3)?,
        filesystem_id: row.get(4)?,
        filesystem_name: row.get(5)?,
        fsmap_epoch: decode_u32(row.get(6)?, 6)?,
        mdsmap_epoch: decode_u32(row.get(7)?, 7)?,
        descriptor_state: row.get(8)?,
        metadata_pool_id: row.get(9)?,
        expected_replica_count: decode_u32(row.get(10)?, 10)?,
        namespace_input_sha256: row.get(11)?,
        namespace_projection_sha256: row.get(12)?,
        namespace_assembly_sha256: row.get(13)?,
        source_capability: row.get(14)?,
        namespace_schema_version: decode_u32(row.get(15)?, 15)?,
        decoder_profile: row.get(16)?,
        journal_boundary_sha256: row.get(17)?,
        lineage_fingerprint: row.get(18)?,
    })
}

fn decode_u32(value: i64, index: usize) -> rusqlite::Result<u32> {
    u32::try_from(value).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Integer,
            "integer is outside the u32 range".into(),
        )
    })
}
