use crate::post_import_service::{events, CapabilityOutcome, CapabilityWork};
use analysis_core::analysis_service::{
    self, AnalysisServiceError, AnalysisSourceReadRuntime, ImportSourceExtractionRequest,
};
use domain::{CaseId, DataSourceId, DataSourcePlatform};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub(super) fn run(
    work: &CapabilityWork,
    conn: &rusqlite::Connection,
    cancel: &Arc<AtomicBool>,
    platform: DataSourcePlatform,
) -> CapabilityOutcome {
    let Some(categories) = crate::import_pipeline::artifact_extraction_categories(platform) else {
        return CapabilityOutcome::terminal("notApplicable");
    };
    let runtime = AnalysisSourceReadRuntime::with_bitlocker_runtime(Arc::clone(&work.bitlocker));
    let mut last_progress = Instant::now() - Duration::from_millis(250);
    let mut event_error = None;
    let result = analysis_service::run_import_source_extraction(
        ImportSourceExtractionRequest {
            case_conn: conn,
            case_root: &work.case_root,
            case_id: &CaseId(work.row.case_id.clone()),
            source: &DataSourceId(work.row.data_source_id.clone()),
            categories,
            runtime: &runtime,
            cancel_token: Arc::clone(cancel),
            run_id: &format!(
                "post-import-{}-{}",
                work.row.data_source_id, work.row.revision
            ),
        },
        |progress| {
            work.progress.analysis(work, &progress);
            if last_progress.elapsed() >= Duration::from_millis(250) && event_error.is_none() {
                last_progress = Instant::now();
                if let Err(error) = events::progress(
                    work,
                    progress.processed_candidates,
                    progress.total_candidates,
                    &progress.label,
                ) {
                    event_error = Some(error);
                    cancel.store(true, Ordering::Release);
                }
            }
        },
    );
    if let Some(error) = event_error {
        tracing::error!(error = %error, "post-import progress publication failed");
        return CapabilityOutcome::failed("EVENT_PUBLICATION_FAILED");
    }
    match result {
        Ok(execution) => artifact_capability_state(
            execution.retryable_failure_count,
            execution.dto.warnings.len(),
        ),
        Err(AnalysisServiceError::Cancelled) => CapabilityOutcome::terminal("cancelled"),
        Err(error) => {
            tracing::error!(error = %error, "post-import artifact extraction failed");
            CapabilityOutcome::failed("EXTRACTION_FAILED")
        }
    }
}

fn artifact_capability_state(retryable_failures: u64, warning_count: usize) -> CapabilityOutcome {
    if retryable_failures == 0 {
        return CapabilityOutcome::terminal("ready");
    }
    CapabilityOutcome {
        state: "partial",
        detail: Some(format!(
            "retryableFailures={retryable_failures} warnings={warning_count}"
        )),
        error_code: Some("EXTRACTION_PARTIAL"),
        timeline_events: 0,
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/post_import_service/artifacts.rs"]
mod tests;
