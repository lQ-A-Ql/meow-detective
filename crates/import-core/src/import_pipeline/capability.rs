//! Task-confirmed capability row writes for the detached (catalog-only) import
//! path.
//!
//! Import-core already writes control-plane rows directly through
//! `persistence-sqlite` repositories (`StorageControlRepo`, `JobRepo`,
//! `TimelineProjectionRepo`); this helper follows the same boundary and must
//! not call `app-services` (forbidden dependency). Rows are conservative: the
//! row is written after the owning marker/decision, never before.

use persistence_sqlite::repositories::source_capability_repo::{
    SourceCapabilityRepo, SourceCapabilityRow,
};
use rusqlite::Connection;
use transport::CommandError;

/// Wire state values shared with `crates/transport/src/dto/capability.rs`.
pub(crate) const QUEUED: &str = "queued";
pub(crate) const NOT_APPLICABLE: &str = "notApplicable";
pub(crate) const READY: &str = "ready";

/// Write (or upsert) a task-confirmed capability row for one source.
///
/// `derived = 0` marks the row as task-confirmed; revision is carried over and
/// only bumped on a transition into `ready`, mirroring
/// `capability_service::mark_capability_state`.
pub(crate) fn mark_capability(
    conn: &Connection,
    case_id: &domain::CaseId,
    source_id: &domain::DataSourceId,
    capability: &str,
    state: &str,
    detail: Option<&str>,
    error_code: Option<&str>,
) -> Result<(), CommandError> {
    let repo = SourceCapabilityRepo::new(conn);
    let existing = repo
        .get(&case_id.0, &source_id.0, capability)
        .map_err(CommandError::from_service_error)?;
    let revision = match existing.as_ref() {
        Some(row) if state == READY && row.state != READY => row.revision + 1,
        Some(row) => row.revision,
        None => 0,
    };
    let row = SourceCapabilityRow {
        case_id: case_id.0.clone(),
        data_source_id: source_id.0.clone(),
        capability: capability.to_string(),
        state: state.to_string(),
        revision,
        progress_done: 0,
        progress_total: None,
        phase: None,
        detail: detail.map(str::to_string),
        error_code: error_code.map(str::to_string),
        derived: 0,
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    repo.upsert(&row).map_err(CommandError::from_service_error)
}
