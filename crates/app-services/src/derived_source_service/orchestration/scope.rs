use std::sync::atomic::AtomicBool;

use domain::{CaseId, CephScopeId, DataSourceId};
use persistence_sqlite::repositories::ceph_osd_repo::CephOsdRepo;
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;

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
    let summary = crate::cluster_service::require_ceph_scope(conn, &case_id.0, ceph_scope_id)
        .map_err(|error| DerivedSourceError::InconsistentState(error.to_string()))?;
    Ok(CephScopeState {
        status: summary.status,
        member_count: summary.member_count as usize,
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
