use crate::analysis_service::AnalysisServiceError;
use domain::DataSourceId;
use std::path::Path;

pub(super) fn request(case_root: &Path, source: &DataSourceId) -> Result<(), AnalysisServiceError> {
    let layout = persistence_source::fjall::SourceBuildLayout::for_case_source(
        case_root,
        &source.0,
        "analysis-gc",
    )
    .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    let published = layout.published_path();
    let directory = published.parent().expect("source generation has a parent");
    persistence_source::publication::request_analysis_generation_gc(directory)?;
    Ok(())
}
