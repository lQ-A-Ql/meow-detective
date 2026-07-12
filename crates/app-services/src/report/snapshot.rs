use super::{source_analysis, ReportAnalysis, ReportCorrelation, ReportError, ReportGovernance};
use rusqlite::Connection;
use std::path::Path;

pub(crate) fn current_analysis(conn: &Connection) -> Result<ReportAnalysis, ReportError> {
    let system_info =
        crate::analysis_service::extract_system_info_for_case(conn, |file_id, max_bytes| {
            crate::file_service::read_file_header_by_id(conn, file_id, max_bytes)
        });
    let files = crate::analysis_service::collect_file_entries(conn)
        .map_err(|err| ReportError::Other(err.to_string()))?;
    let classifications = crate::analysis_service::classify_files_by_magic(
        &files,
        crate::analysis_service::DEFAULT_SAMPLE_SIZE,
        |file_id| {
            crate::file_service::read_file_header_by_id(
                conn,
                file_id,
                crate::analysis_service::MAGIC_HEADER_LIMIT,
            )
        },
    );

    Ok(ReportAnalysis::Single {
        system_info: Box::new(system_info),
        classifications,
    })
}

pub(crate) fn open_ready_source_connections(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
) -> Result<Vec<(domain::DataSourceId, Connection)>, ReportError> {
    Ok(
        source_analysis::open_ready_source_connections(case_conn, case_root, case_id)?
            .into_iter()
            .map(|source| (source.data_source_id, source.connection))
            .collect(),
    )
}

pub(crate) fn current_correlation(conn: &Connection) -> Result<ReportCorrelation, ReportError> {
    Ok(ReportCorrelation {
        snapshot: crate::correlation::get_correlation_snapshot(conn)?,
    })
}

pub(crate) fn current_correlation_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
) -> Result<ReportCorrelation, ReportError> {
    Ok(ReportCorrelation {
        snapshot: crate::correlation::get_correlation_snapshot_for_case(
            case_conn, case_root, case_id,
        )?,
    })
}

pub(crate) fn current_governance(
    conn: &Connection,
    case_id: &str,
) -> Result<ReportGovernance, ReportError> {
    Ok(ReportGovernance {
        snapshot: crate::v2_governance_service::get_v2_governance_snapshot(conn, case_id)?,
    })
}

pub(crate) fn current_governance_for_case(
    conn: &Connection,
    case_root: &Path,
    case_id: &str,
) -> Result<ReportGovernance, ReportError> {
    Ok(ReportGovernance {
        snapshot: crate::v2_governance_service::get_v2_governance_snapshot_for_case(
            conn, case_root, case_id,
        )?,
    })
}

pub(crate) fn correlation_confidence_str(
    value: &transport::dto::CorrelationConfidenceDto,
) -> &'static str {
    match value {
        transport::dto::CorrelationConfidenceDto::Direct => "direct",
        transport::dto::CorrelationConfidenceDto::Strong => "strong",
        transport::dto::CorrelationConfidenceDto::Weak => "weak",
        transport::dto::CorrelationConfidenceDto::Heuristic => "heuristic",
    }
}
