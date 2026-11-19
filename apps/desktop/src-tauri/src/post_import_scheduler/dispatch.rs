//! Task execution: timeline projection rebuild and host-platform artifact
//! extraction for one queued capability.

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
use transport::dto::{AnalysisExtractionProgressDto, TimelineProjectionStateDto};
use transport::ServiceErrorCategory;

use super::events::{emit_capability_progress, emit_timeline_updated, write_row};
use super::QueuedTask;

/// Capability-progress emit throttle; the final frame always goes out.
pub(super) const PROGRESS_THROTTLE: Duration = Duration::from_millis(250);

pub(super) fn run_timeline(task: &QueuedTask, conn: &rusqlite::Connection) {
    let cancel = Arc::new(AtomicBool::new(false));
    let status = rebuild_timeline_projection(
        conn,
        &task.case_root,
        &domain::CaseId(task.case_id.clone()),
        Some(&task.source_id),
        TimelineProjectionPolicy::default(),
        &cancel,
    );
    match status {
        Ok(projection) => {
            let entry = projection
                .sources
                .iter()
                .find(|entry| entry.data_source_id == task.source_id);
            match entry {
                Some(entry) => match entry.state {
                    TimelineProjectionStateDto::Complete => {
                        write_row(task, "ready", None, None);
                        if entry.inserted_events > 0 {
                            emit_timeline_updated(task.app.as_ref(), entry.inserted_events);
                        }
                    }
                    TimelineProjectionStateDto::Deferred => {
                        write_row(task, "deferred", entry.reason.as_deref(), None);
                    }
                    TimelineProjectionStateDto::Building => {
                        write_row(task, "queued", Some("projection still building"), None);
                    }
                },
                None => write_row(
                    task,
                    "failed",
                    Some("projection status missing for source"),
                    None,
                ),
            }
        }
        Err(TimelineServiceError::RebuildInProgress { .. }) => {
            // Another worker owns the rebuild; leave the row untouched.
            tracing::info!(
                case_id = %task.case_id,
                source_id = %task.source_id,
                "timeline rebuild already in progress; post-import task leaves row as-is"
            );
        }
        Err(TimelineServiceError::Cancelled) => write_row(task, "cancelled", None, None),
        Err(error) => {
            let error_code = error
                .code()
                .unwrap_or("TIMELINE_REBUILD_FAILED")
                .to_string();
            write_row(task, "failed", Some(&error.to_string()), Some(&error_code));
        }
    }
}

pub(super) fn run_artifacts(task: &QueuedTask, conn: &rusqlite::Connection) {
    let storage =
        match DataSourceRepo::new(conn).find_storage(&DataSourceId(task.source_id.clone())) {
            Ok(Some(storage)) => storage,
            Ok(None) => {
                write_row(task, "failed", Some("source storage record missing"), None);
                return;
            }
            Err(error) => {
                write_row(task, "failed", Some(&error.to_string()), Some("DB"));
                return;
            }
        };
    let platform = DataSourcePlatform::from_storage_str(Some(storage.platform.as_ref()))
        .unwrap_or(DataSourcePlatform::Unknown);
    let cancel = Arc::new(AtomicBool::new(false));
    match platform {
        DataSourcePlatform::Android => run_artifacts_android(task, conn, &cancel),
        DataSourcePlatform::Windows | DataSourcePlatform::Linux => {
            run_artifacts_host(task, conn, &cancel, platform)
        }
        DataSourcePlatform::Unknown => {
            write_row(task, "notApplicable", Some("unsupported platform"), None);
        }
    }
}

fn run_artifacts_android(task: &QueuedTask, conn: &rusqlite::Connection, cancel: &AtomicBool) {
    let runtime = AnalysisSourceReadRuntime::with_bitlocker_runtime(Arc::clone(
        &task.app_state.bitlocker_runtime,
    ));
    let result = analysis_service::run_source_android_analysis_with_cancel(
        conn,
        &task.case_root,
        &domain::CaseId(task.case_id.clone()),
        &DataSourceId(task.source_id.clone()),
        &runtime,
        cancel,
    );
    match result {
        Ok(_) => write_row(task, "ready", None, None),
        Err(AnalysisServiceError::Cancelled) => write_row(task, "cancelled", None, None),
        Err(error) => write_row(task, "failed", Some(&error.to_string()), Some("EXTRACTION")),
    }
}

fn run_artifacts_host(
    task: &QueuedTask,
    conn: &rusqlite::Connection,
    cancel: &Arc<AtomicBool>,
    platform: DataSourcePlatform,
) {
    let Some(categories) = app_services::import_pipeline::artifact_extraction_categories(platform)
    else {
        write_row(task, "notApplicable", None, None);
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
            cancel_token: cancel.clone(),
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
            write_row(task, "ready", None, None);
        }
        Err(AnalysisServiceError::Cancelled) => write_row(task, "cancelled", None, None),
        Err(error) => {
            write_row(task, "failed", Some(&error.to_string()), Some("EXTRACTION"));
        }
    }
}
