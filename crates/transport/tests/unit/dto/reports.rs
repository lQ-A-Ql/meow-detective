use super::*;
use crate::{
    commands::ExportReportRequest,
    dto::{ReportFormatDto, ReportSourceSummaryDto, ReportStatusDto},
};

#[test]
fn report_history_roundtrip_preserves_terminal_status_and_request() {
    for status in [
        ReportStatusDto::Running,
        ReportStatusDto::Completed,
        ReportStatusDto::Failed,
        ReportStatusDto::Cancelled,
    ] {
        let item = ReportHistoryItemDto {
            id: "report-1".into(),
            file_name: "report.html".into(),
            created_by: "examiner".into(),
            created_at: "2026-11-26T00:00:00Z".into(),
            status,
            progress: Some(50),
            request: ExportReportRequest {
                case_id: "case-1".into(),
                format: ReportFormatDto::Html,
                scope: Default::default(),
                data_source_id: None,
            },
            error_code: None,
            error_message: None,
            file_size: None,
            sha256: None,
        };
        let value = serde_json::to_value(&item).unwrap();
        assert!(value.get("fileName").is_some());
        assert!(value.get("errorMessage").is_none());
        let decoded: ReportHistoryItemDto = serde_json::from_value(value).unwrap();
        assert_eq!(decoded.status, status);
        assert_eq!(decoded.request.case_id, "case-1");
    }
}

#[test]
fn source_summary_roundtrip_has_structured_sections() {
    let value = serde_json::to_value(ReportSourceSummaryDto::default()).unwrap();
    assert!(value.get("operatingSystem").is_some());
    assert!(value.get("hardware").is_some());
    assert!(value.get("browsers").is_some());
    assert!(value.get("email").is_some());
    let decoded: ReportSourceSummaryDto = serde_json::from_value(value).unwrap();
    assert_eq!(decoded, ReportSourceSummaryDto::default());
}
