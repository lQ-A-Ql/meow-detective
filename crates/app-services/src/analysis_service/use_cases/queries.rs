use std::path::Path;

use domain::{CaseId, DataSourceId};
use rusqlite::Connection;
use transport::dto::{
    BrowserHistorySummaryDto, EmailExtractionSummaryDto, EvtxEventSummaryDto,
    LinuxArtifactSummaryDto, RegistryExtractionSummaryDto, RegistryStructuredSummaryDto,
};

use super::source::open_ready_analysis_source;
use crate::analysis_service::{
    get_browser_history_summary, get_email_extraction_summary, get_evtx_event_summary,
    get_linux_artifact_summary, get_registry_extraction_summary, get_registry_structured_summary,
    validate_analysis_categories, AnalysisServiceError,
};

fn open_for_capability(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    capability: &str,
) -> Result<super::source::AnalysisSource, AnalysisServiceError> {
    let source = open_ready_analysis_source(case_conn, case_root, case_id, data_source_id)?;
    validate_analysis_categories(source.platform, &[capability])?;
    Ok(source)
}

pub fn get_source_registry_summary(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    offset: u64,
    limit: u32,
) -> Result<RegistryExtractionSummaryDto, AnalysisServiceError> {
    let source = open_for_capability(case_conn, case_root, case_id, data_source_id, "Registry")?;
    get_registry_extraction_summary(&source.connection, offset, limit)
}

pub fn get_source_registry_structured_summary(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<RegistryStructuredSummaryDto, AnalysisServiceError> {
    let source = open_for_capability(case_conn, case_root, case_id, data_source_id, "Registry")?;
    get_registry_structured_summary(&source.connection)
}

macro_rules! paged_summary {
    ($name:ident, $capability:literal, $return_type:ty, $query:path) => {
        pub fn $name(
            case_conn: &Connection,
            case_root: &Path,
            case_id: &CaseId,
            data_source_id: &DataSourceId,
            offset: u64,
            limit: u32,
        ) -> Result<$return_type, AnalysisServiceError> {
            let source =
                open_for_capability(case_conn, case_root, case_id, data_source_id, $capability)?;
            $query(&source.connection, offset, limit)
        }
    };
}

paged_summary!(
    get_source_browser_summary,
    "BrowserHistory",
    BrowserHistorySummaryDto,
    get_browser_history_summary
);
paged_summary!(
    get_source_email_summary,
    "Email",
    EmailExtractionSummaryDto,
    get_email_extraction_summary
);
paged_summary!(
    get_source_evtx_summary,
    "EventLogs",
    EvtxEventSummaryDto,
    get_evtx_event_summary
);
paged_summary!(
    get_source_linux_summary,
    "LinuxArtifacts",
    LinuxArtifactSummaryDto,
    get_linux_artifact_summary
);
