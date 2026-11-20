//! Task execution: timeline projection rebuild and host-platform artifact
//! extraction for one claimed capability.
//!
//! Idempotency pre-checks run before any expensive work: a Complete projection
//! or a task-confirmed ready artifacts row short-circuits to `ready` without
//! re-running. The shared cancel token is signalled by `cancel_source_capability`
//! and observed at chunk boundaries.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use app_services::analysis_service::{
    self, AnalysisServiceError, AnalysisSourceReadRuntime, ImportSourceExtractionRequest,
};
use app_services::timeline_service::{
    rebuild_timeline_projection, TimelineProjectionPolicy, TimelineServiceError,
};
use domain::{DataSourceId, DataSourcePlatform};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use persistence_sqlite::repositories::source_capability_repo::SourceCapabilityRepo;
use transport::dto::{
    AnalysisExtractionProgressDto, TimelineProjectionStateDto, TimelineProjectionStatusDto,
};
use transport::ServiceErrorCategory;

use super::events::{emit_capability_progress, emit_timeline_updated, write_row};
use super::QueuedTask;

/// Capability-progress emit throttle; the final frame always goes out.
pub(super) const PROGRESS_THROTTLE: Duration = Duration::from_millis(250);

pub(super) fn run(task: &QueuedTask, conn: &rusqlite::Connection, token: &Arc<AtomicBool>) {
    match task.capability {
        super::QueuedCapability::Timeline => run_timeline(task, conn, token),
        super::QueuedCapability::Artifacts => run_artifacts(task, conn, token),
    }
}

fn run_timeline(task: &QueuedTask, conn: &rusqlite::Connection, token: &Arc<AtomicBool>) {
    // Idempotency: projection already Complete for this source → ready, no
    // rebuild. The marker itself is the revision source of truth.
    let status = match app_services::timeline_service::get_timeline_projection_status(
        conn,
        &task.case_root,
        &domain::CaseId(task.case_id.clone()),
    ) {
        Ok(status) => Some(status),
        Err(error) => {
            tracing::warn!(error = %error, "timeline status pre-check failed; proceeding to rebuild");
            None
        }
    };
    if let Some(status) = status.as_ref() {
        let complete = status
            .sources
            .iter()
            .find(|entry| entry.data_source_id == task.source_id)
            .map(|entry| entry.state == TimelineProjectionStateDto::Complete)
            .unwrap_or(false);
        if complete {
            write_row(task, "ready", None, None, Some("done"));
            return;
        }
    }
    let status = rebuild_timeline_projection(
        conn,
        &task.case_root,
        &domain::CaseId(task.case_id.clone()),
        Some(&task.source_id),
        TimelineProjectionPolicy::default(),
        token,
    );
    match status {
        Ok(projection) => apply_timeline_projection(task, &projection),
        Err(TimelineServiceError::RebuildInProgress { .. }) => {
            // Another worker owns the rebuild; leave the row untouched.
            tracing::info!(
                case_id = %task.case_id,
                source_id = %task.source_id,
                "timeline rebuild already in progress; post-import task leaves row as-is"
            );
        }
        Err(TimelineServiceError::Cancelled) => {
            write_row(task, "cancelled", None, None, Some("done"))
        }
        Err(error) => {
            let error_code = error
                .code()
                .unwrap_or("TIMELINE_REBUILD_FAILED")
                .to_string();
            write_row(
                task,
                "failed",
                Some(&error.to_string()),
                Some(&error_code),
                Some("done"),
            );
        }
    }
}

/// Translate a completed projection status into the terminal capability row.
fn apply_timeline_projection(task: &QueuedTask, projection: &TimelineProjectionStatusDto) {
    let entry = projection
        .sources
        .iter()
        .find(|entry| entry.data_source_id == task.source_id);
    match entry {
        Some(entry) => match entry.state {
            TimelineProjectionStateDto::Complete => {
                write_row(task, "ready", None, None, Some("done"));
                if entry.inserted_events > 0 {
                    emit_timeline_updated(task.app.as_ref(), entry.inserted_events);
                }
            }
            TimelineProjectionStateDto::Deferred => {
                write_row(
                    task,
                    "deferred",
                    entry.reason.as_deref(),
                    None,
                    Some("done"),
                );
            }
            TimelineProjectionStateDto::Building => {
                write_row(
                    task,
                    "queued",
                    Some("projection still building"),
                    None,
                    None,
                );
            }
        },
        None => write_row(
            task,
            "failed",
            Some("projection status missing for source"),
            None,
            Some("done"),
        ),
    }
}

fn run_artifacts(task: &QueuedTask, conn: &rusqlite::Connection, token: &Arc<AtomicBool>) {
    // Idempotency: no run_id column exists, so a task-confirmed (derived=0)
    // ready row means a prior extraction already succeeded; skip re-running.
    let already_ready = SourceCapabilityRepo::new(conn)
        .get(&task.case_id, &task.source_id, "artifacts")
        .ok()
        .flatten()
        .map(|row| row.derived == 0 && row.state == "ready")
        .unwrap_or(false);
    if already_ready {
        write_row(task, "ready", None, None, Some("done"));
        return;
    }
    let storage =
        match DataSourceRepo::new(conn).find_storage(&DataSourceId(task.source_id.clone())) {
            Ok(Some(storage)) => storage,
            Ok(None) => {
                write_row(
                    task,
                    "failed",
                    Some("source storage record missing"),
                    None,
                    Some("done"),
                );
                return;
            }
            Err(error) => {
                write_row(
                    task,
                    "failed",
                    Some(&error.to_string()),
                    Some("DB"),
                    Some("done"),
                );
                return;
            }
        };
    let platform = DataSourcePlatform::from_storage_str(Some(storage.platform.as_ref()))
        .unwrap_or(DataSourcePlatform::Unknown);
    match platform {
        DataSourcePlatform::Android => run_artifacts_android(task, conn, token),
        DataSourcePlatform::Windows | DataSourcePlatform::Linux => {
            run_artifacts_host(task, conn, token, platform)
        }
        DataSourcePlatform::Unknown => {
            write_row(
                task,
                "notApplicable",
                Some("unsupported platform"),
                None,
                Some("done"),
            );
        }
    }
}

fn run_artifacts_android(task: &QueuedTask, conn: &rusqlite::Connection, token: &Arc<AtomicBool>) {
    let runtime = AnalysisSourceReadRuntime::with_bitlocker_runtime(Arc::clone(
        &task.app_state.bitlocker_runtime,
    ));
    let result = analysis_service::run_source_android_analysis_with_cancel(
        conn,
        &task.case_root,
        &domain::CaseId(task.case_id.clone()),
        &DataSourceId(task.source_id.clone()),
        &runtime,
        token,
    );
    match result {
        Ok(_) => write_row(task, "ready", None, None, Some("done")),
        Err(AnalysisServiceError::Cancelled) => {
            write_row(task, "cancelled", None, None, Some("done"))
        }
        Err(error) => write_row(
            task,
            "failed",
            Some(&error.to_string()),
            Some("EXTRACTION"),
            Some("done"),
        ),
    }
}

/// Translate a finished host-platform extraction into the terminal capability
/// row. A positive `retryable_failure_count` means some candidates failed
/// transiently while partial results still committed: the row becomes `partial`
/// (never `ready`) with a readable `retryableFailures=N warnings=M` reason and
/// the `EXTRACTION_PARTIAL` marker. A zero count yields `ready` with no detail;
/// informational warnings alone do not downgrade the row.
pub(super) fn artifact_capability_state(
    retryable_failures: u64,
    warning_count: usize,
) -> (&'static str, Option<String>, Option<&'static str>) {
    if retryable_failures > 0 {
        let detail = format!(
            "extraction finished with retryable failures: retryableFailures={retryable_failures} warnings={warning_count}"
        );
        return ("partial", Some(detail), Some("EXTRACTION_PARTIAL"));
    }
    ("ready", None, None)
}

fn run_artifacts_host(
    task: &QueuedTask,
    conn: &rusqlite::Connection,
    token: &Arc<AtomicBool>,
    platform: DataSourcePlatform,
) {
    let Some(categories) = app_services::import_pipeline::artifact_extraction_categories(platform)
    else {
        write_row(task, "notApplicable", None, None, Some("done"));
        return;
    };
    let runtime = AnalysisSourceReadRuntime::with_bitlocker_runtime(Arc::clone(
        &task.app_state.bitlocker_runtime,
    ));
    let mut last_progress = Instant::now() - PROGRESS_THROTTLE;
    let progress_app = task.app.clone();
    let progress_case = task.case_id.clone();
    let progress_source = task.source_id.clone();
    let result = analysis_service::run_import_source_extraction(
        ImportSourceExtractionRequest {
            case_conn: conn,
            case_root: &task.case_root,
            case_id: &domain::CaseId(task.case_id.clone()),
            source: &DataSourceId(task.source_id.clone()),
            categories,
            runtime: &runtime,
            cancel_token: token.clone(),
            run_id: &format!("post-import-{}", task.source_id),
        },
        |update: AnalysisExtractionProgressDto| {
            let now = Instant::now();
            if now.duration_since(last_progress) >= PROGRESS_THROTTLE {
                last_progress = now;
                emit_capability_progress(
                    progress_app.as_ref(),
                    &progress_case,
                    &progress_source,
                    "artifacts",
                    update.processed_candidates,
                    Some(update.total_candidates),
                    Some(&update.label),
                );
            }
        },
    );
    match result {
        Ok(execution) => {
            emit_capability_progress(
                task.app.as_ref(),
                &task.case_id,
                &task.source_id,
                "artifacts",
                execution.dto.artifact_count,
                None,
                Some("complete"),
            );
            let (state, detail, error_code) = artifact_capability_state(
                execution.retryable_failure_count,
                execution.dto.warnings.len(),
            );
            write_row(task, state, detail.as_deref(), error_code, Some("done"));
        }
        Err(AnalysisServiceError::Cancelled) => {
            write_row(task, "cancelled", None, None, Some("done"))
        }
        Err(error) => {
            write_row(
                task,
                "failed",
                Some(&error.to_string()),
                Some("EXTRACTION"),
                Some("done"),
            );
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/post_import_scheduler/dispatch.rs"]
mod tests;
