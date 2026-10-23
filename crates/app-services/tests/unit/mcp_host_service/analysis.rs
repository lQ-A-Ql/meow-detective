use super::analysis_error;
use crate::{analysis_service::AnalysisServiceError, mcp_host_service::McpHostServiceError};

#[test]
fn unsupported_platform_is_reported_as_not_applicable() {
    let error = analysis_error(AnalysisServiceError::Unsupported(
        "analysis capability `linux` belongs to LINUX, not WINDOWS".to_string(),
    ));

    assert!(matches!(error, McpHostServiceError::UnsupportedPlatform(_)));
    assert!(error.to_string().contains("平台不支持"));
    assert!(error.to_string().contains("LINUX"));
}

#[test]
fn analysis_not_found_keeps_the_source_identifier() {
    let error = analysis_error(AnalysisServiceError::NotFound(
        "Data source",
        "missing-source".to_string(),
    ));

    assert!(matches!(error, McpHostServiceError::NotFoundDetail(_)));
    assert!(error.to_string().contains("missing-source"));
}
