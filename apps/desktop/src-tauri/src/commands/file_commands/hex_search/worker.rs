//! Worker that runs one whole-file hex search scan on a background task.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use domain::CaseId;
use transport::dto::HexSearchPhaseDto;

use crate::state::AppState;

const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);

pub(super) struct HexSearchJob {
    pub case_root: PathBuf,
    pub case_id: String,
    pub file_id: String,
    pub task_id: String,
    pub pattern: Vec<u8>,
    pub max_matches: usize,
    pub cancel_token: Arc<AtomicBool>,
}

pub(super) fn run_hex_search(app_state: AppState, job: HexSearchJob) -> Result<(), String> {
    app_state.hex_search_jobs.update_if_current(
        &job.case_id,
        &job.file_id,
        &job.task_id,
        |status| {
            if status.phase == HexSearchPhaseDto::Queued {
                status.phase = HexSearchPhaseDto::Running;
            }
        },
    );
    let connection =
        match app_services::connection::open_existing_case_db(&job.case_root.join("app.db")) {
            Ok(connection) => connection,
            Err(_) => return fail(&app_state, &job, "hex_search_failed"),
        };
    let case_id = CaseId(job.case_id.clone());
    let mut last_emit: Option<Instant> = None;
    let mut on_progress = |progress: app_services::file_service::HexScanProgress| {
        if !progress.truncated && last_emit.is_some_and(|at| at.elapsed() < PROGRESS_INTERVAL) {
            return;
        }
        last_emit = Some(Instant::now());
        app_state.hex_search_jobs.update_if_current(
            &job.case_id,
            &job.file_id,
            &job.task_id,
            |status| {
                if status.phase != HexSearchPhaseDto::Cancelling {
                    status.phase = HexSearchPhaseDto::Running;
                }
                status.scanned_bytes = progress.scanned_bytes;
                status.total_bytes = progress.total_bytes;
                status.matches = progress.matches;
                status.truncated = progress.truncated;
            },
        );
    };
    let result = app_services::file_service::scan_preview_session_bytes(
        app_services::file_service::HexScanSessionRequest {
            bitlocker_runtime: &app_state.bitlocker_runtime,
            registry: &app_state.preview_runtime,
            case_conn: &connection,
            case_root: &job.case_root,
            case_id: &case_id,
            file_id: &job.file_id,
        },
        &job.pattern,
        job.max_matches,
        &job.cancel_token,
        &mut on_progress,
    );
    let cancelled = job.cancel_token.load(Ordering::Acquire);
    match result {
        Ok(outcome) if outcome.cancelled || cancelled => {
            finish(
                &app_state,
                &job,
                Some(&outcome),
                HexSearchPhaseDto::Cancelled,
                None,
            );
            Ok(())
        }
        Ok(outcome) => {
            finish(
                &app_state,
                &job,
                Some(&outcome),
                HexSearchPhaseDto::Completed,
                None,
            );
            Ok(())
        }
        Err(_) if cancelled => {
            finish(&app_state, &job, None, HexSearchPhaseDto::Cancelled, None);
            Ok(())
        }
        Err(error) => {
            let code = error_code(&error);
            finish(
                &app_state,
                &job,
                None,
                HexSearchPhaseDto::Failed,
                Some(code),
            );
            Err(code.to_string())
        }
    }
}

fn finish(
    app_state: &AppState,
    job: &HexSearchJob,
    outcome: Option<&app_services::file_service::HexScanOutcome>,
    phase: HexSearchPhaseDto,
    error: Option<&str>,
) {
    app_state.hex_search_jobs.update_if_current(
        &job.case_id,
        &job.file_id,
        &job.task_id,
        |status| {
            if let Some(outcome) = outcome {
                status.matches = outcome.matches.clone();
                status.scanned_bytes = outcome.scanned_bytes;
                status.total_bytes = outcome.total_bytes;
                status.truncated = outcome.truncated;
            }
            status.phase = phase;
            status.error = error.map(str::to_string);
        },
    );
}

fn fail(app_state: &AppState, job: &HexSearchJob, code: &str) -> Result<(), String> {
    finish(app_state, job, None, HexSearchPhaseDto::Failed, Some(code));
    Err(code.to_string())
}

fn error_code(error: &app_services::file_service::FileServiceError) -> &'static str {
    use app_services::file_service::FileServiceError;
    match error {
        FileServiceError::NotFound(_) => "source_unavailable",
        FileServiceError::Unsupported(_) => "unsupported_source",
        FileServiceError::Io(_) => "io_error",
        FileServiceError::InvalidInput(_) => "invalid_pattern",
        _ => "hex_search_failed",
    }
}
