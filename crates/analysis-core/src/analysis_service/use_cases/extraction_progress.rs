use crate::analysis_service::extraction::ExtractionProgressUpdate;
use domain::{CaseId, DataSourceId};
use transport::dto::AnalysisExtractionProgressDto;

pub(super) fn analysis_progress_dto(
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    run_id: &str,
    update: ExtractionProgressUpdate,
) -> AnalysisExtractionProgressDto {
    AnalysisExtractionProgressDto {
        run_id: run_id.to_string(),
        case_id: case_id.0.clone(),
        data_source_id: data_source_id.0.clone(),
        category: update.category,
        label: update.label,
        phase: update.phase,
        total_candidates: update.total_candidates,
        processed_candidates: update.processed_candidates,
        structured_candidates: update.structured_candidates,
        unsupported_candidates: update.unsupported_candidates,
        text_fallback_candidates: update.text_fallback_candidates,
        warning_candidates: update.warning_candidates,
        checkpoint_hit_count: update.checkpoint_hit_count,
        artifact_count: update.artifact_count,
        timeline_event_count: update.timeline_event_count,
        current_path: update.current_path,
        detail: update.detail,
        work: update.work,
    }
}
