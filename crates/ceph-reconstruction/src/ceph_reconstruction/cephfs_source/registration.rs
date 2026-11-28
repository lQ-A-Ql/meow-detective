use domain::{
    structure::{AcquisitionId, SourceRevisionContent},
    CaseId, DataSource, DataSourceId, DataSourceKind,
};
use persistence_sqlite::repositories::{
    ceph_fs_lineage_repo::{
        insert_cephfs_lineage_in_transaction, CephFsDerivedLineageAggregate,
        CephFsDerivedLineageRepo,
    },
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    source_revision_repo::SourceRevisionRepo,
};

use super::{CephFsSourceError, CephFsSourceResult};

pub(super) fn ensure_registration(
    case_conn: &rusqlite::Connection,
    case_id: &CaseId,
    source: &DataSource,
    storage: &DataSourceStorage,
    lineage: &CephFsDerivedLineageAggregate,
) -> CephFsSourceResult<DataSource> {
    let existing = DataSourceRepo::new(case_conn)
        .find_by_case(case_id)?
        .into_iter()
        .find(|candidate| candidate.id == source.id);
    match existing {
        Some(existing) => {
            validate_existing(case_conn, &existing, source, storage, lineage)?;
            bind_parent_inputs(case_conn, case_id, &existing.id, lineage)?;
            Ok(existing)
        }
        None => {
            insert_registration(case_conn, case_id, source, storage, lineage)?;
            DataSourceRepo::new(case_conn)
                .find_by_case(case_id)?
                .into_iter()
                .find(|candidate| candidate.id == source.id)
                .ok_or_else(|| {
                    CephFsSourceError::InconsistentState(
                        "CephFS registration disappeared after commit".to_string(),
                    )
                })
        }
    }
}

fn insert_registration(
    case_conn: &rusqlite::Connection,
    case_id: &CaseId,
    source: &DataSource,
    storage: &DataSourceStorage,
    lineage: &CephFsDerivedLineageAggregate,
) -> CephFsSourceResult<()> {
    let transaction = case_conn
        .unchecked_transaction()
        .map_err(persistence_sqlite::DbError::from)?;
    DataSourceRepo::new(&transaction).insert_with_storage(case_id, source, storage)?;
    insert_cephfs_lineage_in_transaction(&transaction, lineage)?;
    bind_parent_inputs(&transaction, case_id, &source.id, lineage)?;
    transaction
        .commit()
        .map_err(persistence_sqlite::DbError::from)?;
    Ok(())
}

fn bind_parent_inputs(
    conn: &rusqlite::Connection,
    case_id: &CaseId,
    source_id: &DataSourceId,
    lineage: &CephFsDerivedLineageAggregate,
) -> CephFsSourceResult<()> {
    let mut source_ids = lineage
        .pools
        .iter()
        .flat_map(|pool| pool.sources.iter())
        .map(|source| source.source_data_source_id.as_str())
        .chain(
            lineage
                .map_provenance
                .iter()
                .map(|source| source.source_data_source_id.as_str()),
        )
        .collect::<Vec<_>>();
    source_ids.sort_unstable();
    source_ids.dedup();
    let revisions = SourceRevisionRepo::new(conn);
    let inputs = source_ids
        .into_iter()
        .map(|parent| {
            revisions
                .current(&case_id.0, parent)
                .map(|(reference, _)| reference)
        })
        .collect::<Result<Vec<_>, _>>()?;
    revisions.bind_inputs(&case_id.0, &source_id.0, &inputs)?;
    Ok(())
}

fn validate_existing(
    case_conn: &rusqlite::Connection,
    existing: &DataSource,
    desired: &DataSource,
    desired_storage: &DataSourceStorage,
    desired_lineage: &CephFsDerivedLineageAggregate,
) -> CephFsSourceResult<()> {
    if existing.kind != DataSourceKind::CephFs || existing.source_path != desired.source_path {
        return Err(CephFsSourceError::InconsistentState(
            "existing data-source registration does not identify this filesystem".to_string(),
        ));
    }
    let storage = DataSourceRepo::new(case_conn)
        .find_storage(&existing.id)?
        .ok_or_else(|| {
            CephFsSourceError::InconsistentState(
                "existing CephFS source has no storage metadata".to_string(),
            )
        })?;
    if storage.storage_model != desired_storage.storage_model
        || storage.source_db_rel_path != desired_storage.source_db_rel_path
        || storage.platform != desired_storage.platform
        || storage.profile != desired_storage.profile
    {
        return Err(CephFsSourceError::InconsistentState(
            "existing CephFS source storage binding changed".to_string(),
        ));
    }
    let stored_lineage = CephFsDerivedLineageRepo::new(case_conn)
        .find_by_data_source(&existing.id.0)?
        .ok_or_else(|| {
            CephFsSourceError::InconsistentState(
                "existing CephFS source has no lineage".to_string(),
            )
        })?;
    if stored_lineage != *desired_lineage {
        return Err(CephFsSourceError::InconsistentState(
            "existing CephFS source lineage changed".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn import_state(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
) -> CephFsSourceResult<String> {
    DataSourceRepo::new(case_conn)
        .find_storage(data_source_id)?
        .map(|storage| storage.import_state)
        .ok_or_else(|| {
            CephFsSourceError::InconsistentState(
                "CephFS source storage metadata disappeared".to_string(),
            )
        })
}

pub(super) fn bind_derived_revision(
    conn: &rusqlite::Connection,
    case_id: &CaseId,
    source_id: &DataSourceId,
    logical_size: u64,
    metadata_identity: String,
) -> CephFsSourceResult<()> {
    let transaction = conn
        .is_autocommit()
        .then(|| conn.unchecked_transaction())
        .transpose()
        .map_err(persistence_sqlite::DbError::from)?;
    let revisions = SourceRevisionRepo::new(conn);
    let inputs = revisions.inputs(&case_id.0, &source_id.0)?;
    let acquisition = AcquisitionId(format!(
        "ceph-fs:{}",
        inputs
            .iter()
            .map(|input| input.source_id.as_str())
            .collect::<Vec<_>>()
            .join("\u{1f}")
    ));
    revisions.bind(
        &case_id.0,
        &source_id.0,
        &SourceRevisionContent::derived(acquisition, logical_size, metadata_identity),
        true,
    )?;
    let aggregate = CephFsDerivedLineageRepo::new(conn)
        .find_by_data_source(&source_id.0)?
        .ok_or_else(|| CephFsSourceError::InconsistentState("CephFS lineage is missing".into()))?;
    let lineage = aggregate.lineage;
    super::super::bind_ceph_object_version(
        conn,
        &case_id.0,
        &source_id.0,
        super::super::cephfs_native_object_id(
            &lineage.parent_ceph_scope_id,
            &lineage.cluster_identity,
            lineage.filesystem_id,
        ),
        &lineage.filesystem_name,
        &lineage.lineage_fingerprint,
        domain::structure::NativeVersion::CephFs {
            filesystem_id: lineage.filesystem_id,
            fsmap_epoch: lineage.fsmap_epoch,
            mdsmap_epoch: lineage.mdsmap_epoch,
            namespace_input_sha256: lineage.namespace_input_sha256,
            journal_boundary_sha256: lineage.journal_boundary_sha256,
        },
    )?;
    if let Some(transaction) = transaction {
        transaction
            .commit()
            .map_err(persistence_sqlite::DbError::from)?;
    }
    Ok(())
}
