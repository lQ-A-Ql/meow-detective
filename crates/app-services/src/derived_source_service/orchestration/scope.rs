use std::sync::atomic::AtomicBool;

use domain::{CaseId, CephScopeId, DataSourceId};
use persistence_sqlite::repositories::ceph_osd_repo::CephOsdRepo;
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use rusqlite::OptionalExtension;

use super::super::{ensure_not_cancelled, DerivedSourceError, DerivedSourceResult};

pub(super) struct CephScopeState {
    pub(super) status: String,
    pub(super) member_count: usize,
}

pub(super) fn load_ceph_scope(
    conn: &rusqlite::Connection,
    case_id: &CaseId,
    ceph_scope_id: &CephScopeId,
) -> DerivedSourceResult<CephScopeState> {
    let row = conn
        .query_row(
            "SELECT status, evidence_completeness, identity_state, case_id
             FROM linux_topology_scopes
             WHERE id = ?1 AND scope_kind = 'ceph'",
            [&ceph_scope_id.0],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()
        .map_err(persistence_sqlite::DbError::from)
        .map_err(DerivedSourceError::Database)?;
    let (status, completeness, identity_state, owner_case_id) =
        row.ok_or_else(|| DerivedSourceError::ScopeNotFound(ceph_scope_id.0.clone()))?;
    if owner_case_id != case_id.0
        || completeness != "complete"
        || !matches!(identity_state.as_str(), "candidate" | "proven")
    {
        return Err(DerivedSourceError::IncompleteScope);
    }
    let member_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM linux_topology_memberships WHERE scope_id = ?1",
            [&ceph_scope_id.0],
            |row| row.get(0),
        )
        .map_err(persistence_sqlite::DbError::from)?;
    Ok(CephScopeState {
        status,
        member_count: member_count.max(0) as usize,
    })
}

pub(super) fn reconstruction_parent_ids(
    case_conn: &rusqlite::Connection,
    parent_ids: &[DataSourceId],
    cancel_token: &AtomicBool,
) -> DerivedSourceResult<Vec<DataSourceId>> {
    let repo = DataSourceRepo::new(case_conn);
    let mut reconstruction_sources = Vec::new();
    for data_source_id in parent_ids {
        ensure_not_cancelled(cancel_token)?;
        let storage = repo.find_storage(data_source_id)?.ok_or_else(|| {
            DerivedSourceError::InconsistentState(format!(
                "Ceph scope member {} is missing storage metadata",
                data_source_id.0
            ))
        })?;
        if storage.import_state == "ready_metadata" {
            reconstruction_sources.push(data_source_id.clone());
        }
    }
    Ok(reconstruction_sources)
}

pub(super) fn has_osd_inventory(
    case_conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    parent_ids: &[DataSourceId],
    cancel_token: &AtomicBool,
) -> DerivedSourceResult<bool> {
    for source_id in parent_ids {
        ensure_not_cancelled(cancel_token)?;
        let source = crate::source_db::open_reconstruction_source_by_id(
            case_conn, case_root, case_id, source_id,
        )
        .map_err(|error| {
            DerivedSourceError::Database(persistence_sqlite::DbError::System(error.to_string()))
        })?;
        if CephOsdRepo::new(&source.connection)
            .find_by_data_source(&source_id.0)?
            .iter()
            .any(|inventory| inventory.whoami.is_some())
        {
            return Ok(true);
        }
    }
    Ok(false)
}
