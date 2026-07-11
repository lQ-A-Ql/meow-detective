use std::path::Path;

use domain::{CaseId, DataSourceId, DataSourcePlatform};
use rusqlite::Connection;
use transport::dto::{AnalysisFileClassificationDto, AnalysisSystemInfoDto};

use super::ReportError;

pub(crate) enum ReportAnalysis {
    Single {
        system_info: Box<AnalysisSystemInfoDto>,
        classifications: Vec<AnalysisFileClassificationDto>,
    },
    PerSource(Vec<ReportSourceAnalysis>),
}

pub(crate) struct ReportSourceAnalysis {
    pub(crate) data_source_id: DataSourceId,
    pub(crate) platform: DataSourcePlatform,
    pub(crate) system_info: Option<AnalysisSystemInfoDto>,
    pub(crate) classifications: Vec<AnalysisFileClassificationDto>,
}

pub(crate) fn unavailable_windows_system_info(analysis: &ReportAnalysis) -> bool {
    matches!(
        analysis,
        ReportAnalysis::PerSource(sources)
            if !sources
                .iter()
                .any(|source| source.platform == DataSourcePlatform::Windows)
    )
}

pub(crate) fn current_analysis_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
) -> Result<ReportAnalysis, ReportError> {
    let mut analyses = Vec::new();

    for source in open_ready_source_connections(case_conn, case_root, case_id)? {
        let header_cache = crate::file_service::FileHeaderReadCache::new(case_id.0.clone());
        let system_info = (source.platform == DataSourcePlatform::Windows).then(|| {
            crate::analysis_service::extract_system_info_for_case(
                &source.connection,
                |file_id, max_bytes| {
                    header_cache.read_file_header_by_id(&source.connection, file_id, max_bytes)
                },
            )
        });

        let files = crate::analysis_service::collect_file_entries(&source.connection)
            .map_err(|error| ReportError::Other(error.to_string()))?;
        let classifications = crate::analysis_service::classify_files_by_magic(
            &files,
            crate::analysis_service::DEFAULT_SAMPLE_SIZE,
            |file_id| {
                header_cache.read_file_header_by_id(
                    &source.connection,
                    file_id,
                    crate::analysis_service::MAGIC_HEADER_LIMIT,
                )
            },
        );
        analyses.push(ReportSourceAnalysis {
            data_source_id: source.data_source_id,
            platform: source.platform,
            system_info,
            classifications,
        });
    }

    Ok(ReportAnalysis::PerSource(analyses))
}

pub(super) fn open_ready_source_connections(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
) -> Result<Vec<crate::source_db::ReadySourceConnection>, ReportError> {
    let sources = crate::source_db::ready_data_sources(case_conn, case_id)?;
    sources
        .into_iter()
        .map(|(source, _)| {
            crate::source_db::open_ready_source_by_id(case_conn, case_root, case_id, &source.id)
                .map_err(ReportError::from)
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/unit/report/source_analysis_test.rs"]
mod tests;
