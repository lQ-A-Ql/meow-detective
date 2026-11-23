use crate::post_import_service::{CapabilityOutcome, CapabilityWork};
use analysis_core::analysis_service::{self, AnalysisServiceError, AnalysisSourceReadRuntime};
use domain::{CaseId, DataSourceId};
use std::sync::{atomic::AtomicBool, Arc};
use transport::dto::AnalysisParseStatusDto;

pub(super) fn run(
    work: &CapabilityWork,
    conn: &rusqlite::Connection,
    cancel: &Arc<AtomicBool>,
) -> CapabilityOutcome {
    let runtime = AnalysisSourceReadRuntime::with_bitlocker_runtime(Arc::clone(&work.bitlocker));
    match analysis_service::run_source_android_analysis_with_cancel(
        conn,
        &work.case_root,
        &CaseId(work.row.case_id.clone()),
        &DataSourceId(work.row.data_source_id.clone()),
        &runtime,
        cancel,
    ) {
        Ok(result) if result.status == AnalysisParseStatusDto::Partial => CapabilityOutcome {
            state: "partial",
            detail: Some(format!("warnings={}", result.warnings.len())),
            error_code: Some("EXTRACTION_PARTIAL"),
            timeline_events: 0,
        },
        Ok(_) => CapabilityOutcome::terminal("ready"),
        Err(AnalysisServiceError::Cancelled) => CapabilityOutcome::terminal("cancelled"),
        Err(error) => {
            tracing::error!(error = %error, "post-import Android extraction failed");
            CapabilityOutcome::failed("EXTRACTION_FAILED")
        }
    }
}
