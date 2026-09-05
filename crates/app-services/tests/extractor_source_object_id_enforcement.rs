//! Enforcement harness: every extractor-produced Artifact and TimelineEvent
//! must carry a `source_object_id` equal to the input file identifier.
//!
//! This test uses both synthetic byte fixtures and the checked-in tiny registry
//! hives so that the invariant can be verified without relying on private real-
//! world samples.

mod fixture_builder;

use app_services::analysis_service::EvidenceCandidate;
use app_services::{analysis_service::extract_registry_candidate, artifact_service};
use artifacts_core::VecSink;
use chrono::{TimeZone, Utc};
use domain::FileEntryId;
use fixture_builder::{build_lnk, build_prefetch_v30, build_recycle_bin_i};

fn fid(s: &str) -> FileEntryId {
    FileEntryId(s.to_string())
}

fn assert_source_object_id(sink: &VecSink, file_id: &FileEntryId, extractor: &str) {
    for artifact in &sink.artifacts {
        assert_eq!(
            artifact.source_object_id.as_ref(),
            Some(file_id),
            "{extractor} artifact {} is missing source_object_id (expected {})",
            artifact.id.0,
            file_id.0
        );
    }
    for event in &sink.timeline_events {
        assert_eq!(
            event.source_object_id, file_id.0,
            "{extractor} timeline event {} has wrong source_object_id (expected {})",
            event.id.0, file_id.0
        );
    }
}

fn assert_outcome_source_object_id(
    artifacts: &[domain::Artifact],
    events: &[domain::TimelineEvent],
    file_id: &FileEntryId,
    parser: &str,
) {
    for artifact in artifacts {
        assert_eq!(
            artifact.source_object_id.as_ref(),
            Some(file_id),
            "{parser} artifact {} is missing source_object_id (expected {})",
            artifact.id.0,
            file_id.0
        );
    }
    for event in events {
        assert_eq!(
            event.source_object_id, file_id.0,
            "{parser} timeline event {} has wrong source_object_id (expected {})",
            event.id.0, file_id.0
        );
    }
}

#[test]
fn prefetch_extractor_sets_source_object_id() {
    let file_id = fid("pf-src-001");
    let data = build_prefetch_v30("CMD.EXE", 5, &[]);
    let registry = artifact_service::create_registry();
    let mut sink = VecSink::new();
    artifact_service::run_extractors_on_file(
        &registry,
        &file_id,
        "C:/Windows/Prefetch/CMD.EXE-DEADBEEF.pf",
        Box::new(std::io::Cursor::new(data)),
        &mut sink,
    )
    .unwrap();

    assert!(
        !sink.artifacts.is_empty() || !sink.timeline_events.is_empty(),
        "Prefetch fixture should produce at least one output"
    );
    assert_source_object_id(&sink, &file_id, "PrefetchExtractor");
}

#[test]
fn lnk_extractor_sets_source_object_id() {
    let file_id = fid("lnk-src-001");
    let ct = Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
    let wt = Utc.with_ymd_and_hms(2024, 1, 16, 8, 0, 0).unwrap();
    let data = build_lnk(
        Some("C:\\Windows\\System32\\cmd.exe"),
        Some(ct),
        Some(wt),
        1024,
    );
    let registry = artifact_service::create_registry();
    let mut sink = VecSink::new();
    artifact_service::run_extractors_on_file(
        &registry,
        &file_id,
        "C:/Users/alice/Desktop/shortcut.lnk",
        Box::new(std::io::Cursor::new(data)),
        &mut sink,
    )
    .unwrap();

    assert!(
        !sink.artifacts.is_empty(),
        "LNK fixture should produce at least one artifact"
    );
    assert_source_object_id(&sink, &file_id, "LnkExtractor");
}

#[test]
fn recycle_bin_extractor_sets_source_object_id() {
    let file_id = fid("rb-src-001");
    let dt = Utc.with_ymd_and_hms(2024, 6, 15, 10, 30, 0).unwrap();
    let data = build_recycle_bin_i("C:\\Users\\alice\\Documents\\secret.docx", 65536, dt);
    let registry = artifact_service::create_registry();
    let mut sink = VecSink::new();
    artifact_service::run_extractors_on_file(
        &registry,
        &file_id,
        "C:/$Recycle.Bin/$IA1B2C3D4E5.exe",
        Box::new(std::io::Cursor::new(data)),
        &mut sink,
    )
    .unwrap();

    assert!(
        !sink.artifacts.is_empty() || !sink.timeline_events.is_empty(),
        "RecycleBin fixture should produce at least one output"
    );
    assert_source_object_id(&sink, &file_id, "RecycleBinExtractor");
}

#[test]
fn canonical_registry_extraction_sets_source_object_id() {
    let system_path = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/fixtures/public-small/logical/Windows/System32/config/SYSTEM"
    ));
    let software_path = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/fixtures/public-small/logical/Windows/System32/config/SOFTWARE"
    ));

    let system_bytes = std::fs::read(system_path).expect("read SYSTEM fixture");
    let software_bytes = std::fs::read(software_path).expect("read SOFTWARE fixture");

    let system_candidate = EvidenceCandidate {
        file_id: fid("hive-system-001"),
        data_source_id: "ds-1".to_string(),
        partition_index: None,
        path: "C:/Windows/System32/config/SYSTEM".to_string(),
        size: system_bytes.len() as u64,
        encrypted: false,
        content_identity: "test:system".to_string(),
        companions: Vec::new(),
        modified_at: None,
        evidence_kind: "registry_hive".to_string(),
        parser: "registry.system_info".to_string(),
        category: "SystemInformation".to_string(),
    };

    let software_candidate = EvidenceCandidate {
        file_id: fid("hive-software-001"),
        data_source_id: "ds-1".to_string(),
        partition_index: None,
        path: "C:/Windows/System32/config/SOFTWARE".to_string(),
        size: software_bytes.len() as u64,
        encrypted: false,
        content_identity: "test:software".to_string(),
        companions: Vec::new(),
        modified_at: None,
        evidence_kind: "registry_hive".to_string(),
        parser: "registry.system_info".to_string(),
        category: "SystemInformation".to_string(),
    };

    let system_outcome =
        extract_registry_candidate(&system_candidate, &system_bytes, None, None, None);
    let software_outcome =
        extract_registry_candidate(&software_candidate, &software_bytes, None, None, None);

    assert!(
        !system_outcome.artifacts.is_empty() || !system_outcome.timeline_events.is_empty(),
        "SYSTEM fixture should produce at least one output"
    );
    assert!(
        !software_outcome.artifacts.is_empty() || !software_outcome.timeline_events.is_empty(),
        "SOFTWARE fixture should produce at least one output"
    );

    assert_outcome_source_object_id(
        &system_outcome.artifacts,
        &system_outcome.timeline_events,
        &system_candidate.file_id,
        "registry.system_info (SYSTEM)",
    );
    assert_outcome_source_object_id(
        &software_outcome.artifacts,
        &software_outcome.timeline_events,
        &software_candidate.file_id,
        "registry.system_info (SOFTWARE)",
    );
}
