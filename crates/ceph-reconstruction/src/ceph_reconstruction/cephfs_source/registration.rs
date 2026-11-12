use domain::{CaseId, DataSource, DataSourceId, DataSourceKind};
use persistence_sqlite::repositories::{
    ceph_fs_lineage_repo::{
        insert_cephfs_lineage_in_transaction, CephFsDerivedLineageAggregate,
        CephFsDerivedLineageRepo,
    },
    datasource_repo::{DataSourceRepo, DataSourceStorage},
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
    transaction
        .commit()
        .map_err(persistence_sqlite::DbError::from)?;
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
