use super::ReportError;
use persistence_sqlite::repositories::report_repo::{ReportRecord, ReportRepo};
use rusqlite::Connection;
use transport::dto::{ReportHistoryItemDto, ReportStatusDto};

pub fn get_report_history(
    connection: &Connection,
    case_id: &str,
) -> Result<Vec<ReportHistoryItemDto>, ReportError> {
    ReportRepo::new(connection)
        .list_by_case(case_id)?
        .into_iter()
        .map(to_dto)
        .collect()
}

pub(super) fn to_dto(record: ReportRecord) -> Result<ReportHistoryItemDto, ReportError> {
    let status = match record.status.as_str() {
        "running" => ReportStatusDto::Running,
        "completed" => ReportStatusDto::Completed,
        "failed" => ReportStatusDto::Failed,
        "cancelled" => ReportStatusDto::Cancelled,
        _ => return Err(ReportError::Other("invalid report history status".into())),
    };
    Ok(ReportHistoryItemDto {
        id: record.id,
        file_name: record.file_name,
        created_by: record.created_by,
        created_at: record.created_at,
        status,
        progress: record.progress,
        request: serde_json::from_str(&record.request_json)
            .map_err(|error| ReportError::Other(error.to_string()))?,
        error_code: record.error_code,
        error_message: record.error_message,
        file_size: record.file_size,
        sha256: record.sha256,
    })
}
