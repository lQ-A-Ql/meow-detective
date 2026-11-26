use super::{source_request, McpHostAnalysisQueryContext, McpHostServiceError};
use serde_json::Value;

pub(super) fn execute(
    context: &McpHostAnalysisQueryContext<'_>,
    arguments: &Value,
) -> Result<Value, McpHostServiceError> {
    let source = source_request(arguments)?;
    let request = transport::commands::ExportReportRequest {
        case_id: context.query.case_meta.id.0.clone(),
        format: transport::dto::ReportFormatDto::Json,
        scope: Default::default(),
        data_source_id: Some(source.data_source_id),
    };
    let document =
        crate::report::read_report_document_for_case(&crate::report::ReportExportContext {
            connection: context.query.connection,
            case: context.query.case_meta,
            case_root: context.query.case_root,
            request: &request,
            cancel: &std::sync::atomic::AtomicBool::new(false),
        })?;
    serde_json::to_value(document).map_err(McpHostServiceError::from)
}
