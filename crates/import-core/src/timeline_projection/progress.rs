use super::TimelineProjectionError;
use rusqlite::Connection;
use std::sync::atomic::AtomicBool;

pub use transport::dto::TaskProgressWorkDto as TimelineWork;
pub type TimelineProgressCallback<'a> = dyn Fn(TimelineWork, u64, Option<u64>) + 'a;

pub(super) fn prepare_generation(
    conn: &Connection,
    source: &str,
    cancel: &AtomicBool,
    progress: Option<&TimelineProgressCallback<'_>>,
) -> Result<analysis_core::analysis_service::StagedSourceGeneration, TimelineProjectionError> {
    analysis_core::analysis_service::stage_source_generation_with_progress(
        conn,
        source,
        cancel,
        |update| {
            if let Some(progress) = progress {
                progress(TimelineWork::TimelineWorkspace, update.records_copied, None);
            }
        },
    )
    .map_err(|error| match error {
        analysis_core::analysis_service::AnalysisServiceError::Cancelled => {
            TimelineProjectionError::Cancelled
        }
        other => TimelineProjectionError::Other(other.to_string()),
    })
}
