use domain::DataSourceId;
use persistence_sqlite::repositories::processing_phase_repo::ProcessingPhase;
use serde_json::json;

use super::phase_runner::{PhaseClaim, ProcessingPhaseAttempt, ProcessingPhaseRunner};
use crate::derived_source_service::MaterializedRbdSource;

pub(in crate::derived_source_service) fn begin_catalog_phase(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    input_fingerprint: &str,
) -> Result<PhaseClaim, persistence_sqlite::DbError> {
    ProcessingPhaseRunner::new(case_conn, data_source_id, input_fingerprint)
        .claim(ProcessingPhase::Catalog)
}

pub(in crate::derived_source_service) fn complete_catalog_phase(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    input_fingerprint: &str,
    attempt: &ProcessingPhaseAttempt,
    summary: &MaterializedRbdSource,
) -> Result<(), persistence_sqlite::DbError> {
    let stats = json!({
        "materializerVersion": crate::derived_source_catalog::CATALOG_MATERIALIZER_VERSION,
        "recordCount": summary.file_count,
        "directoryCount": summary.directory_count,
        "totalSize": summary.total_size,
        "createdCount": summary.created_count,
        "modifiedCount": summary.modified_count,
        "accessedCount": summary.accessed_count,
        "changedCount": summary.changed_count,
        "catalogDigest": summary.catalog_digest,
    })
    .to_string();
    ProcessingPhaseRunner::new(case_conn, data_source_id, input_fingerprint)
        .ready(attempt, &stats)
        .map(|_| ())
}

pub(in crate::derived_source_service) fn start_catalog_heartbeat(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    input_fingerprint: &str,
    attempt: &ProcessingPhaseAttempt,
) -> Result<super::phase_runner::ProcessingPhaseHeartbeat, persistence_sqlite::DbError> {
    ProcessingPhaseRunner::new(case_conn, data_source_id, input_fingerprint)
        .start_heartbeat(attempt)
}

pub(in crate::derived_source_service) fn refresh_catalog_claim(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    input_fingerprint: &str,
    attempt: &ProcessingPhaseAttempt,
) -> Result<(), persistence_sqlite::DbError> {
    ProcessingPhaseRunner::new(case_conn, data_source_id, input_fingerprint).refresh(attempt)
}

pub(in crate::derived_source_service) fn fail_catalog_phase(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    input_fingerprint: &str,
    attempt: &ProcessingPhaseAttempt,
    error: &str,
) -> Result<(), persistence_sqlite::DbError> {
    ProcessingPhaseRunner::new(case_conn, data_source_id, input_fingerprint)
        .failed(attempt, error)
        .map(|_| ())
}

pub(in crate::derived_source_service) fn defer_catalog_phase(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    input_fingerprint: &str,
    attempt: &ProcessingPhaseAttempt,
    reason: &str,
) -> Result<(), persistence_sqlite::DbError> {
    ProcessingPhaseRunner::new(case_conn, data_source_id, input_fingerprint)
        .deferred(attempt, "{}", reason)
        .map(|_| ())
}
