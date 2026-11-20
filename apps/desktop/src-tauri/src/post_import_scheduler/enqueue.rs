//! Source lookup and queue admission for the post-import scheduler.

use std::path::{Path, PathBuf};

use domain::CaseId;
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, source_capability_repo::SourceCapabilityRepo,
};
use tauri::AppHandle;

use super::events;
use super::state::admit;
use super::{Admission, Priority, QueuedCapability, QueuedTask};
use crate::state::AppState;

/// Resolve the just-imported data source by its registered `source_path`; the
/// import coordinator returns only the summary message string.
fn resolve_source_id(
    conn: &rusqlite::Connection,
    case_id: &str,
    source_path: &Path,
) -> Option<String> {
    DataSourceRepo::new(conn)
        .find_by_case(&CaseId(case_id.to_string()))
        .ok()?
        .into_iter()
        .find(|source| source.source_path == source_path)
        .map(|source| source.id.0)
}

fn read_revision(conn: &rusqlite::Connection, case_id: &str, source_id: &str, wire: &str) -> i64 {
    SourceCapabilityRepo::new(conn)
        .get(case_id, source_id, wire)
        .ok()
        .flatten()
        .map(|row| row.revision)
        .unwrap_or(0)
}

/// Identity tuple locating one schedulable capability work item; the runtime
/// fields (`app_state`, `app`, `priority`) are supplied by the admission path.
struct TaskIdentity {
    case_id: String,
    case_root: PathBuf,
    source_id: String,
    capability: QueuedCapability,
    revision: i64,
}

fn build_task(
    app_state: &AppState,
    app: Option<&AppHandle>,
    identity: TaskIdentity,
    priority: Priority,
) -> QueuedTask {
    QueuedTask {
        app_state: app_state.clone(),
        app: app.cloned(),
        case_id: identity.case_id,
        case_root: identity.case_root,
        source_id: identity.source_id,
        capability: identity.capability,
        priority,
        revision: identity.revision,
    }
}

/// Read the queued/pending `timeline`/`artifacts` rows for one source and admit
/// them as P1 auto work. Called from the import job's Completed branch after
/// `job-completed`; never blocks the job. An identical task already queued or
/// running is deduplicated by admission.
pub(crate) fn enqueue_for_source(
    app_state: &AppState,
    app: Option<&AppHandle>,
    case_id: &str,
    case_root: &Path,
    source_path: &Path,
) {
    let db_path = case_root.join("app.db");
    let conn = match app_services::connection::open_existing_case_db(&db_path) {
        Ok(conn) => conn,
        Err(error) => {
            tracing::warn!(
                case_id,
                error = %error,
                "post-import scheduler: cannot open case DB for enqueue"
            );
            return;
        }
    };
    let Some(source_id) = resolve_source_id(&conn, case_id, source_path) else {
        tracing::warn!(
            case_id,
            "post-import scheduler: source row not found for enqueue"
        );
        return;
    };
    let candidates = [
        (QueuedCapability::Timeline, "timeline"),
        (QueuedCapability::Artifacts, "artifacts"),
    ];
    for (capability, wire) in candidates {
        let row = SourceCapabilityRepo::new(&conn)
            .get(case_id, &source_id, wire)
            .ok()
            .flatten();
        let queued = row
            .as_ref()
            .map(|entry| matches!(entry.state.as_str(), "queued" | "pending"))
            .unwrap_or(false);
        if !queued {
            continue;
        }
        let revision = row.map(|entry| entry.revision).unwrap_or(0);
        let admitted = admit(build_task(
            app_state,
            app,
            TaskIdentity {
                case_id: case_id.to_string(),
                case_root: case_root.to_path_buf(),
                source_id: source_id.clone(),
                capability,
                revision,
            },
            Priority::Auto,
        ));
        if matches!(admitted, Admission::Accepted) {
            events::mark_admitted(case_id, case_root, &source_id, capability.wire());
        }
    }
}

/// After case open, re-admit every `queued`/`pending` executable row of the
/// active case as P1 auto work. `requeue_running` already flipped stale
/// `running` rows to `queued` inside `case_service::open_case`; rows for
/// non-active cases are never scanned.
pub(crate) fn enqueue_case_pending(
    app_state: &AppState,
    app: Option<&AppHandle>,
    case_id: &str,
    case_root: &Path,
) {
    let db_path = case_root.join("app.db");
    let conn = match app_services::connection::open_existing_case_db(&db_path) {
        Ok(conn) => conn,
        Err(error) => {
            tracing::warn!(
                case_id,
                error = %error,
                "post-import scheduler: cannot open case DB for replay enqueue"
            );
            return;
        }
    };
    let pending = match SourceCapabilityRepo::new(&conn).list_pending(case_id) {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(error = %error, "post-import scheduler: cannot list pending rows");
            return;
        }
    };
    for row in pending {
        let Some(capability) = QueuedCapability::parse(&row.capability) else {
            continue;
        };
        let admitted = admit(build_task(
            app_state,
            app,
            TaskIdentity {
                case_id: case_id.to_string(),
                case_root: case_root.to_path_buf(),
                source_id: row.data_source_id.clone(),
                capability,
                revision: row.revision,
            },
            Priority::Auto,
        ));
        if matches!(admitted, Admission::Accepted) {
            events::mark_admitted(case_id, case_root, &row.data_source_id, capability.wire());
        }
    }
}

/// Admit a P0 user-retry task. The caller has already flipped the row to
/// `queued`; admission reports whether the task was accepted, deduped, or
/// rejected for a full P0 slot.
pub(crate) fn retry_admission(
    app_state: &AppState,
    app: Option<&AppHandle>,
    case_id: &str,
    case_root: &Path,
    source_id: &str,
    capability: QueuedCapability,
) -> Admission {
    let db_path = case_root.join("app.db");
    let revision = match app_services::connection::open_existing_case_db(&db_path) {
        Ok(conn) => read_revision(&conn, case_id, source_id, capability.wire()),
        Err(error) => {
            tracing::warn!(error = %error, "post-import scheduler: retry revision read failed");
            0
        }
    };
    let admitted = admit(build_task(
        app_state,
        app,
        TaskIdentity {
            case_id: case_id.to_string(),
            case_root: case_root.to_path_buf(),
            source_id: source_id.to_string(),
            capability,
            revision,
        },
        Priority::UserRetry,
    ));
    if matches!(admitted, Admission::Accepted) {
        events::mark_admitted(case_id, case_root, source_id, capability.wire());
    }
    admitted
}
