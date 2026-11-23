//! Shared mutations for image structure reports.

use super::limits;
use super::types::{
    ImageAnalysisReport, ImageFinding, ImageIssueSeverity, ImageSegmentReport, ImageWarning,
};

pub(super) fn push_segment(
    report: &mut ImageAnalysisReport,
    kind: String,
    offset: u64,
    length: u64,
    detail: Option<String>,
) {
    let Some(structure) = report.structure.as_mut() else {
        return;
    };
    structure.segment_count += 1;
    if structure.segments.len() < limits::REPORTED_SEGMENT_LIMIT {
        structure.segments.push(ImageSegmentReport {
            kind,
            offset,
            length,
            detail,
        });
    } else {
        structure.segments_truncated = true;
    }
}

pub(super) fn push_finding(
    report: &mut ImageAnalysisReport,
    code: &'static str,
    severity: ImageIssueSeverity,
    offset: Option<u64>,
    detail: Option<String>,
) {
    if let Some(structure) = report.structure.as_mut() {
        structure.findings.push(ImageFinding {
            code,
            severity,
            offset,
            detail,
        });
    }
}

pub(super) fn push_warning(
    report: &mut ImageAnalysisReport,
    code: &'static str,
    severity: ImageIssueSeverity,
    message: &'static str,
    offset: Option<u64>,
) {
    report.warnings.push(ImageWarning {
        code,
        severity,
        message,
        offset,
    });
}

pub(super) fn set_declared_end(report: &mut ImageAnalysisReport, end: u64) {
    if let Some(structure) = report.structure.as_mut() {
        structure.declared_end = Some(end);
    }
}

pub(super) fn mark_truncated(report: &mut ImageAnalysisReport, code: &'static str, offset: u64) {
    if let Some(structure) = report.structure.as_mut() {
        structure.truncated = true;
        structure.declared_end = None;
        structure.trailing_bytes = None;
        structure.findings.push(ImageFinding {
            code,
            severity: ImageIssueSeverity::Error,
            offset: Some(offset),
            detail: None,
        });
    }
}
