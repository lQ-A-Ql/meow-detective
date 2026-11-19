//! Source lookup and queue admission for the post-import scheduler.

use std::path::Path;

use domain::CaseId;
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, source_capability_repo::SourceCapabilityRepo,
};
use tauri::AppHandle;

use super::{condvar, state, worker_loop, QueuedCapability, QueuedTask};
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

/// Read the queued/pending `timeline`/`artifacts` rows for one source and enqueue
/// them. Called from the import job's Completed branch after `job-completed`;
/// never blocks the job. Rows that are not queued/pending are ignored, and an
/// already-queued identical task is deduplicated.
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
    let repo = SourceCapabilityRepo::new(&conn);
    let candidates = [
        (QueuedCapability::Timeline, "timeline"),
        (QueuedCapability::Artifacts, "artifacts"),
    ];
    let mut guard = match state().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    for (capability, wire) in candidates {
        let queued = repo
            .get(case_id, &source_id, wire)
            .ok()
            .flatten()
            .map(|row| matches!(row.state.as_str(), "queued" | "pending"))
            .unwrap_or(false);
        if !queued {
            continue;
        }
        if guard.queue.iter().any(|task| {
            task.case_id == case_id && task.source_id == source_id && task.capability == capability
        }) {
            continue;
        }
        guard.queue.push_back(QueuedTask {
            app_state: app_state.clone(),
            app: app.cloned(),
            case_id: case_id.to_string(),
            case_root: case_root.to_path_buf(),
            source_id: source_id.clone(),
            capability,
        });
    }
    if !guard.queue.is_empty() && !guard.worker_started {
        guard.worker_started = true;
        std::thread::Builder::new()
            .name("post-import-scheduler".to_string())
            .spawn(worker_loop)
            .expect("post-import scheduler thread must start");
    }
    condvar().notify_one();
}
