use super::*;
use domain::FileEntryId;
use evidence_access::file_service::{
    ExifBlockReport, ExifFields, ImageAnalysisReport, ImageAnalysisStatus, ImageFinding,
    ImageFormat, ImageIssueSeverity, ImageSegmentReport, ImageStructureHashes,
    ImageStructureReport, ImageWarning,
};

fn candidate() -> EvidenceCandidate {
    EvidenceCandidate {
        file_id: FileEntryId("file-1".to_string()),
        data_source_id: "source-1".to_string(),
        partition_index: Some(2),
        path: "[P2]/evidence/photo.jpg".to_string(),
        size: 65_536,
        encrypted: false,
        content_identity: "test:file-1".to_string(),
        companions: Vec::new(),
        modified_at: None,
        evidence_kind: "image_metadata".to_string(),
        parser: "image.metadata.container".to_string(),
        category: "ImageMetadata".to_string(),
    }
}

fn structure_with_trailing() -> ImageStructureReport {
    ImageStructureReport {
        declared_end: Some(65_024),
        trailing_bytes: Some(512),
        truncated: false,
        segments: vec![ImageSegmentReport {
            kind: "jpeg.sos".to_string(),
            offset: 128,
            length: 64_000,
            detail: Some("scan".to_string()),
        }],
        segment_count: 1,
        segments_truncated: false,
        findings: vec![ImageFinding {
            code: "image.trailing-data",
            severity: ImageIssueSeverity::Warning,
            offset: Some(65_024),
            detail: Some("512 trailing bytes after the declared end".to_string()),
        }],
        hashes: ImageStructureHashes {
            whole_file_sha256: Some("a".repeat(64)),
            trailing_sha256: Some("b".repeat(64)),
            trailing_hashed_bytes: Some(512),
        },
    }
}

fn report_with_exif() -> ImageAnalysisReport {
    ImageAnalysisReport {
        format: Some(ImageFormat::Jpeg),
        status: ImageAnalysisStatus::Present,
        dimensions: Some((4032, 3024)),
        exif: ExifFields {
            orientation: Some(6),
            make: Some("Acme".to_string()),
            model: Some("P1".to_string()),
            date_time_original: Some("2026:10:14 12:34:56".to_string()),
            latitude: Some(31.23),
            longitude: Some(121.47),
            ..ExifFields::default()
        },
        exif_blocks: vec![ExifBlockReport {
            source: "jpeg.app1",
            offset: 20,
            length: 128,
            decoded: true,
            field_count: 23,
            warnings: Vec::new(),
        }],
        warnings: Vec::new(),
        structure: Some(structure_with_trailing()),
        bytes_read: 65_536,
    }
}

#[test]
fn exif_report_projects_one_artifact_without_timeline_events() {
    let outcome = extract(&candidate(), &report_with_exif());

    assert_eq!(outcome.artifacts.len(), 1);
    assert!(outcome.timeline_events.is_empty());
    let artifact = &outcome.artifacts[0];
    assert!(!artifact.id.0.is_empty());
    assert_eq!(artifact.family, "ImageMetadata");
    assert_eq!(
        artifact.source_object_id.as_ref().map(|id| id.0.as_str()),
        Some("file-1")
    );
    assert_eq!(
        artifact.extractor_id.as_deref(),
        Some("image.metadata.container")
    );
    assert_eq!(artifact.extractor_version.as_deref(), Some("2.0.0"));
    assert_eq!(artifact.confidence, Some(0.85));
    assert_eq!(artifact.title, "Image metadata: photo.jpg");
    assert!(artifact.summary.contains("JPEG 4032x3024"));
    assert!(artifact.summary.contains("EXIF present (23 fields)"));
    assert!(artifact.summary.contains("trailing 512 bytes"));
    assert!(artifact.summary.contains("1 findings"));

    let attrs = &artifact.attrs;
    assert_eq!(attrs["format"], serde_json::json!("jpeg"));
    assert_eq!(attrs["status"], serde_json::json!("present"));
    assert_eq!(attrs["width"], serde_json::json!(4032));
    assert_eq!(attrs["height"], serde_json::json!(3024));
    assert_eq!(attrs["orientation"], serde_json::json!(6));
    assert_eq!(attrs["make"], serde_json::json!("Acme"));
    assert_eq!(
        attrs["dateTimeOriginal"],
        serde_json::json!("2026:10:14 12:34:56")
    );
    assert_eq!(attrs["latitude"], serde_json::json!(31.23));
    assert_eq!(attrs["dataSourceId"], serde_json::json!("source-1"));
    assert_eq!(
        attrs["sourcePath"],
        serde_json::json!("[P2]/evidence/photo.jpg")
    );
    assert_eq!(attrs["partitionIndex"], serde_json::json!(2));
    assert!(attrs.get("software").is_none());
    assert!(attrs.get("gpsDateTime").is_none());

    let blocks = attrs["exifBlocks"].as_array().expect("exif blocks array");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0]["source"], serde_json::json!("jpeg.app1"));
    assert_eq!(blocks[0]["fieldCount"], serde_json::json!(23));
    assert_eq!(blocks[0]["decoded"], serde_json::json!(true));

    let structure = attrs["structure"].as_object().expect("structure object");
    assert_eq!(structure["declaredEnd"], serde_json::json!(65_024));
    assert_eq!(structure["trailingBytes"], serde_json::json!(512));
    assert_eq!(structure["truncated"], serde_json::json!(false));
    assert_eq!(structure["segmentCount"], serde_json::json!(1));
    assert_eq!(
        structure["findings"][0]["code"],
        serde_json::json!("image.trailing-data")
    );
    assert_eq!(
        structure["findings"][0]["severity"],
        serde_json::json!("warning")
    );
    assert_eq!(
        structure["hashes"]["trailingSha256"],
        serde_json::json!("b".repeat(64))
    );
    assert_eq!(
        structure["hashes"]["trailingHashedBytes"],
        serde_json::json!(512)
    );
}

#[test]
fn clean_image_produces_no_artifact() {
    let report = ImageAnalysisReport {
        format: Some(ImageFormat::Png),
        status: ImageAnalysisStatus::Absent,
        dimensions: Some((16, 16)),
        exif: ExifFields::default(),
        exif_blocks: Vec::new(),
        warnings: Vec::new(),
        structure: Some(ImageStructureReport {
            declared_end: Some(128),
            trailing_bytes: Some(0),
            truncated: false,
            segments: Vec::new(),
            segment_count: 0,
            segments_truncated: false,
            findings: Vec::new(),
            hashes: ImageStructureHashes::default(),
        }),
        bytes_read: 128,
    };

    let outcome = extract(&candidate(), &report);
    assert!(outcome.artifacts.is_empty());
    assert!(outcome.timeline_events.is_empty());
    assert!(outcome.warnings.is_empty());
}

#[test]
fn corrupt_report_with_warning_still_emits_artifact() {
    let report = ImageAnalysisReport {
        format: Some(ImageFormat::Gif),
        status: ImageAnalysisStatus::Corrupt,
        dimensions: None,
        exif: ExifFields::default(),
        exif_blocks: Vec::new(),
        warnings: vec![ImageWarning {
            code: "image.gif.truncated",
            severity: ImageIssueSeverity::Warning,
            message: "GIF ends before the declared trailer",
            offset: Some(64),
        }],
        structure: None,
        bytes_read: 64,
    };

    let outcome = extract(&candidate(), &report);
    assert_eq!(outcome.artifacts.len(), 1);
    assert_eq!(
        outcome.artifacts[0].attrs["status"],
        serde_json::json!("corrupt")
    );
    let warnings = outcome.artifacts[0].attrs["warnings"]
        .as_array()
        .expect("warnings array");
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0]["severity"], serde_json::json!("warning"));
    assert!(outcome.artifacts[0].summary.contains("EXIF corrupt"));
}

#[test]
fn structure_segments_are_capped_in_artifact_attributes() {
    let mut structure = structure_with_trailing();
    structure.segments = (0..300u64)
        .map(|index| ImageSegmentReport {
            kind: "jpeg.marker".to_string(),
            offset: index,
            length: 1,
            detail: None,
        })
        .collect();
    structure.segment_count = 300;
    structure.segments_truncated = true;
    let mut report = report_with_exif();
    report.structure = Some(structure);

    let outcome = extract(&candidate(), &report);
    let segments = outcome.artifacts[0].attrs["structure"]["segments"]
        .as_array()
        .expect("segments array");
    assert_eq!(segments.len(), MAX_ATTRIBUTE_SEGMENTS);
    assert_eq!(
        outcome.artifacts[0].attrs["structure"]["segmentsTruncated"],
        serde_json::json!(true)
    );
    assert_eq!(
        outcome.artifacts[0].attrs["structure"]["segmentCount"],
        serde_json::json!(300)
    );
}

#[test]
fn android_image_metadata_request_is_typed_unsupported() {
    let result =
        crate::analysis_service::platforms::analyzer_for(domain::DataSourcePlatform::Android)
            .and_then(|analyzer| analyzer.select_capabilities(&["ImageMetadata"]));
    assert!(matches!(
        result,
        Err(crate::analysis_service::error::AnalysisServiceError::Unsupported(_))
    ));
}
