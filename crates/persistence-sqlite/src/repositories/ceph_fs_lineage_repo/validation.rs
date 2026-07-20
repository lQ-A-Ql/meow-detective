use std::collections::HashSet;

use chrono::DateTime;
use rusqlite::{params, Connection};

use crate::connection::{DbError, DbResult};

use super::{cephfs_lineage_fingerprint, CephFsDerivedLineageAggregate};

pub(super) fn validate_aggregate(aggregate: &CephFsDerivedLineageAggregate) -> DbResult<()> {
    let lineage = &aggregate.lineage;
    for value in [
        lineage.derived_data_source_id.as_str(),
        lineage.parent_cluster_id.as_str(),
        lineage.cluster_identity.as_str(),
        lineage.filesystem_identity.as_str(),
        lineage.filesystem_name.as_str(),
        lineage.decoder_profile.as_str(),
    ] {
        validate_text(value)?;
    }
    if lineage.filesystem_id < 0
        || lineage.fsmap_epoch == 0
        || lineage.mdsmap_epoch == 0
        || lineage.descriptor_state != "present"
        || lineage.metadata_pool_id < 0
        || lineage.expected_replica_count == 0
        || !matches!(
            lineage.source_capability.as_str(),
            "metadata-only" | "metadata-browseable" | "bounded-preview"
        )
        || lineage.namespace_schema_version != 1
        || lineage.decoder_profile != "cephfs-namespace-v1"
    {
        return invalid("CephFS lineage identity or schema is invalid");
    }
    validate_sha256(&lineage.namespace_input_sha256)?;
    validate_sha256(&lineage.namespace_projection_sha256)?;
    validate_sha256(&lineage.namespace_assembly_sha256)?;
    validate_sha256(&lineage.lineage_fingerprint)?;
    if let Some(value) = lineage.journal_boundary_sha256.as_deref() {
        validate_sha256(value)?;
    }
    validate_pools(aggregate)?;
    validate_map_provenance(aggregate)?;
    if cephfs_lineage_fingerprint(aggregate) != lineage.lineage_fingerprint {
        return invalid("CephFS lineage fingerprint does not match canonical records");
    }
    Ok(())
}

fn validate_pools(aggregate: &CephFsDerivedLineageAggregate) -> DbResult<()> {
    let expected = aggregate.lineage.expected_replica_count as usize;
    let mut pool_ids = HashSet::new();
    let mut data_ordinal = 0u32;
    let mut metadata_count = 0usize;
    for pool in &aggregate.pools {
        if pool.pool_id < 0 || !pool_ids.insert(pool.pool_id) {
            return invalid("CephFS pool identity is invalid or duplicated");
        }
        match pool.role.as_str() {
            "metadata" if pool.ordinal == 0 => {
                metadata_count += 1;
                if pool.pool_id != aggregate.lineage.metadata_pool_id {
                    return invalid("CephFS metadata pool does not match lineage");
                }
            }
            "data" if pool.ordinal == data_ordinal => data_ordinal += 1,
            _ => return invalid("CephFS pool role or ordinal is invalid"),
        }
        if pool.sources.len() < expected {
            return invalid("CephFS pool replica coverage is not closed");
        }
        let mut source_ids = HashSet::new();
        let mut inventory_ids = HashSet::new();
        for (index, source) in pool.sources.iter().enumerate() {
            if usize::try_from(source.ordinal).ok() != Some(index)
                || source.source_data_source_id == aggregate.lineage.derived_data_source_id
                || !source_ids.insert(source.source_data_source_id.as_str())
                || !inventory_ids.insert(source.inventory_id.as_str())
            {
                return invalid("CephFS pool source identity is invalid or duplicated");
            }
            validate_text(&source.source_data_source_id)?;
            validate_text(&source.inventory_id)?;
        }
    }
    if metadata_count != 1 || data_ordinal == 0 {
        return invalid("CephFS lineage requires one metadata pool and at least one data pool");
    }
    Ok(())
}

fn validate_map_provenance(aggregate: &CephFsDerivedLineageAggregate) -> DbResult<()> {
    if aggregate.map_provenance.is_empty() {
        return invalid("CephFS map provenance is empty");
    }
    let mut identities = HashSet::new();
    for (index, item) in aggregate.map_provenance.iter().enumerate() {
        if usize::try_from(item.ordinal).ok() != Some(index)
            || !identities.insert((
                item.source_data_source_id.as_str(),
                item.inventory_id.as_str(),
            ))
            || DateTime::parse_from_rfc3339(&item.captured_at).is_err()
        {
            return invalid("CephFS map provenance is invalid or duplicated");
        }
        validate_text(&item.source_data_source_id)?;
        validate_text(&item.inventory_id)?;
        validate_sha256(&item.raw_fsmap_sha256)?;
        validate_sha256(&item.raw_mdsmap_sha256)?;
    }
    Ok(())
}

pub(super) fn validate_ownership(
    conn: &Connection,
    aggregate: &CephFsDerivedLineageAggregate,
) -> DbResult<()> {
    let lineage = &aggregate.lineage;
    let derived_matches: bool = conn.query_row(
        "SELECT COUNT(*) = 1
         FROM data_sources AS derived
         JOIN data_source_clusters AS cluster
           ON cluster.id = ?2 AND cluster.case_id = derived.case_id
         WHERE derived.id = ?1 AND derived.kind = 'ceph_fs'",
        params![lineage.derived_data_source_id, lineage.parent_cluster_id],
        |row| row.get(0),
    )?;
    if !derived_matches {
        return invalid("CephFS derived source and cluster do not share a case");
    }
    for source_id in aggregate
        .pools
        .iter()
        .flat_map(|pool| {
            pool.sources
                .iter()
                .map(|source| &source.source_data_source_id)
        })
        .chain(
            aggregate
                .map_provenance
                .iter()
                .map(|item| &item.source_data_source_id),
        )
    {
        let source_matches: bool = conn.query_row(
            "SELECT COUNT(*) = 1
             FROM data_sources AS source
             JOIN data_source_clusters AS cluster
               ON cluster.id = ?2 AND cluster.case_id = source.case_id
             WHERE source.id = ?1 AND source.cluster_id = cluster.id",
            params![source_id, lineage.parent_cluster_id],
            |row| row.get(0),
        )?;
        if !source_matches {
            return invalid("CephFS lineage source is not a member of the parent cluster");
        }
    }
    Ok(())
}

fn validate_text(value: &str) -> DbResult<()> {
    if value.trim().is_empty() || value.contains('\0') {
        return invalid("CephFS lineage text is empty or contains a NUL byte");
    }
    Ok(())
}

fn validate_sha256(value: &str) -> DbResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return invalid("CephFS lineage digest is not canonical SHA-256");
    }
    Ok(())
}

pub(super) fn invalid<T>(message: impl Into<String>) -> DbResult<T> {
    Err(DbError::System(message.into()))
}
