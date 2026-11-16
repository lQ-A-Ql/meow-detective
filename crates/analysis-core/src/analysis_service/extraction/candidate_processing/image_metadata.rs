//! Projects one bounded image-structure report into an `ImageMetadata` artifact.

use crate::analysis_service::artifact_builders::{base_attrs, make_artifact};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::extraction::ExtractionOutcome;
use evidence_access::file_service::{
    ExifBlockReport, ImageAnalysisReport, ImageAnalysisStatus, ImageFinding, ImageIssueSeverity,
    ImageSegmentReport, ImageStructureReport, ImageWarning,
};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

const IMAGE_METADATA_EXTRACTOR_ID: &str = "image.metadata.container";
const MAX_ATTRIBUTE_SEGMENTS: usize = 256;

pub(super) fn extract(
    candidate: &EvidenceCandidate,
    report: &ImageAnalysisReport,
) -> ExtractionOutcome {
    if !report.has_forensic_content() {
        return ExtractionOutcome::default();
    }
    let title = format!("Image metadata: {}", file_name(&candidate.path));
    let summary = report_summary(report);
    let attrs = metadata_attrs(candidate, report);
    let mut outcome = ExtractionOutcome::default();
    outcome.artifacts.push(make_artifact(
        "ImageMetadata",
        title,
        summary,
        candidate,
        IMAGE_METADATA_EXTRACTOR_ID,
        attrs,
    ));
    outcome
}

fn file_name(path: &str) -> &str {
    path.rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or(path)
}

fn report_summary(report: &ImageAnalysisReport) -> String {
    let mut parts = Vec::new();
    if let Some(format) = report.format {
        let mut label = format.as_str().to_ascii_uppercase();
        if let Some((width, height)) = report.dimensions {
            label.push_str(&format!(" {width}x{height}"));
        }
        parts.push(label);
    }
    if let Some(exif) = exif_summary(report) {
        parts.push(exif);
    }
    if let Some(structure) = report.structure.as_ref() {
        if structure.truncated {
            parts.push("truncated".to_string());
        }
        if let Some(trailing) = structure.trailing_bytes.filter(|bytes| *bytes > 0) {
            parts.push(format!("trailing {trailing} bytes"));
        }
        if !structure.findings.is_empty() {
            parts.push(format!("{} findings", structure.findings.len()));
        }
    }
    if parts.is_empty() {
        "image metadata".to_string()
    } else {
        parts.join("; ")
    }
}

fn exif_summary(report: &ImageAnalysisReport) -> Option<String> {
    if report.exif_blocks.iter().any(|block| block.decoded) {
        let fields = report
            .exif_blocks
            .iter()
            .filter(|block| block.decoded)
            .map(|block| u64::from(block.field_count))
            .sum::<u64>();
        return Some(format!("EXIF present ({fields} fields)"));
    }
    match report.status {
        ImageAnalysisStatus::Unsupported => Some("EXIF unsupported".to_string()),
        ImageAnalysisStatus::Corrupt => Some("EXIF corrupt".to_string()),
        ImageAnalysisStatus::Oversized => Some("EXIF oversized".to_string()),
        ImageAnalysisStatus::Present | ImageAnalysisStatus::Absent => None,
    }
}

fn metadata_attrs(
    candidate: &EvidenceCandidate,
    report: &ImageAnalysisReport,
) -> BTreeMap<String, Value> {
    let mut attrs = base_attrs(candidate);
    if let Some(format) = report.format {
        attrs.insert(
            "format".to_string(),
            Value::String(format.as_str().to_string()),
        );
    }
    attrs.insert(
        "status".to_string(),
        Value::String(status_label(report.status).to_string()),
    );
    if let Some((width, height)) = report.dimensions {
        attrs.insert("width".to_string(), Value::from(width));
        attrs.insert("height".to_string(), Value::from(height));
    }
    if let Some(orientation) = report.exif.orientation {
        attrs.insert("orientation".to_string(), Value::from(orientation));
    }
    let exif = &report.exif;
    for (key, value) in [
        ("make", exif.make.as_ref()),
        ("model", exif.model.as_ref()),
        ("software", exif.software.as_ref()),
        ("dateTimeOriginal", exif.date_time_original.as_ref()),
        ("createDate", exif.create_date.as_ref()),
        ("modifyDate", exif.modify_date.as_ref()),
        ("lensModel", exif.lens_model.as_ref()),
        ("gpsDateTime", exif.gps_date_time.as_ref()),
    ] {
        if let Some(value) = value {
            attrs.insert(key.to_string(), Value::String(value.clone()));
        }
    }
    for (key, value) in [
        ("latitude", exif.latitude),
        ("longitude", exif.longitude),
        ("altitude", exif.altitude),
    ] {
        if let Some(value) = value {
            attrs.insert(key.to_string(), Value::from(value));
        }
    }
    let blocks = report
        .exif_blocks
        .iter()
        .map(block_json)
        .collect::<Vec<_>>();
    if !blocks.is_empty() {
        attrs.insert("exifBlocks".to_string(), Value::Array(blocks));
    }
    if !report.warnings.is_empty() {
        let warnings = report.warnings.iter().map(warning_json).collect::<Vec<_>>();
        attrs.insert("warnings".to_string(), Value::Array(warnings));
    }
    if let Some(structure) = report.structure.as_ref() {
        attrs.insert("structure".to_string(), structure_json(structure));
    }
    attrs
}

fn status_label(status: ImageAnalysisStatus) -> &'static str {
    match status {
        ImageAnalysisStatus::Present => "present",
        ImageAnalysisStatus::Absent => "absent",
        ImageAnalysisStatus::Unsupported => "unsupported",
        ImageAnalysisStatus::Corrupt => "corrupt",
        ImageAnalysisStatus::Oversized => "oversized",
    }
}

fn severity_label(severity: ImageIssueSeverity) -> &'static str {
    match severity {
        ImageIssueSeverity::Info => "info",
        ImageIssueSeverity::Warning => "warning",
        ImageIssueSeverity::Error => "error",
    }
}

fn warning_json(warning: &ImageWarning) -> Value {
    let mut map = Map::new();
    map.insert("code".to_string(), Value::String(warning.code.to_string()));
    map.insert(
        "severity".to_string(),
        Value::String(severity_label(warning.severity).to_string()),
    );
    map.insert(
        "message".to_string(),
        Value::String(warning.message.to_string()),
    );
    insert_opt(&mut map, "offset", warning.offset.map(Value::from));
    Value::Object(map)
}

fn block_json(block: &ExifBlockReport) -> Value {
    let mut map = Map::new();
    map.insert(
        "source".to_string(),
        Value::String(block.source.to_string()),
    );
    map.insert("offset".to_string(), Value::from(block.offset));
    map.insert("length".to_string(), Value::from(block.length));
    map.insert("decoded".to_string(), Value::Bool(block.decoded));
    map.insert("fieldCount".to_string(), Value::from(block.field_count));
    if !block.warnings.is_empty() {
        let warnings = block.warnings.iter().map(warning_json).collect::<Vec<_>>();
        map.insert("warnings".to_string(), Value::Array(warnings));
    }
    Value::Object(map)
}

fn structure_json(structure: &ImageStructureReport) -> Value {
    let mut map = Map::new();
    insert_opt(
        &mut map,
        "declaredEnd",
        structure.declared_end.map(Value::from),
    );
    insert_opt(
        &mut map,
        "trailingBytes",
        structure.trailing_bytes.map(Value::from),
    );
    map.insert("truncated".to_string(), Value::Bool(structure.truncated));
    map.insert(
        "segmentCount".to_string(),
        Value::from(structure.segment_count),
    );
    if !structure.segments.is_empty() {
        let segments = structure
            .segments
            .iter()
            .take(MAX_ATTRIBUTE_SEGMENTS)
            .map(segment_json)
            .collect::<Vec<_>>();
        map.insert("segments".to_string(), Value::Array(segments));
    }
    map.insert(
        "segmentsTruncated".to_string(),
        Value::Bool(structure.segments_truncated),
    );
    if !structure.findings.is_empty() {
        let findings = structure
            .findings
            .iter()
            .map(finding_json)
            .collect::<Vec<_>>();
        map.insert("findings".to_string(), Value::Array(findings));
    }
    let mut hashes = Map::new();
    insert_opt(
        &mut hashes,
        "wholeFileSha256",
        structure
            .hashes
            .whole_file_sha256
            .as_ref()
            .map(|hash| Value::String(hash.clone())),
    );
    insert_opt(
        &mut hashes,
        "trailingSha256",
        structure
            .hashes
            .trailing_sha256
            .as_ref()
            .map(|hash| Value::String(hash.clone())),
    );
    insert_opt(
        &mut hashes,
        "trailingHashedBytes",
        structure.hashes.trailing_hashed_bytes.map(Value::from),
    );
    map.insert("hashes".to_string(), Value::Object(hashes));
    Value::Object(map)
}

fn segment_json(segment: &ImageSegmentReport) -> Value {
    let mut map = Map::new();
    map.insert("kind".to_string(), Value::String(segment.kind.clone()));
    map.insert("offset".to_string(), Value::from(segment.offset));
    map.insert("length".to_string(), Value::from(segment.length));
    insert_opt(
        &mut map,
        "detail",
        segment
            .detail
            .as_ref()
            .map(|detail| Value::String(detail.clone())),
    );
    Value::Object(map)
}

fn finding_json(finding: &ImageFinding) -> Value {
    let mut map = Map::new();
    map.insert("code".to_string(), Value::String(finding.code.to_string()));
    map.insert(
        "severity".to_string(),
        Value::String(severity_label(finding.severity).to_string()),
    );
    insert_opt(&mut map, "offset", finding.offset.map(Value::from));
    insert_opt(
        &mut map,
        "detail",
        finding
            .detail
            .as_ref()
            .map(|detail| Value::String(detail.clone())),
    );
    Value::Object(map)
}

fn insert_opt(map: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        map.insert(key.to_string(), value);
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/analysis_service/extraction/image_metadata.rs"]
mod tests;
