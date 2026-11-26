use super::*;
use crate::analysis_service::{extract_registry_candidate, EvidenceCandidate};
use domain::FileEntryId;

#[test]
fn registry_fixture_projection_uses_extracted_system_fields() {
    let root = tempfile::tempdir().unwrap();
    let store = SourceStore::open(root.path().join("source.fjall"), Default::default()).unwrap();
    let bytes = testing::builders::registry::synthetic_software_hive();
    let candidate = EvidenceCandidate {
        file_id: FileEntryId("software".into()),
        data_source_id: "source-1".into(),
        partition_index: None,
        path: "Windows/System32/config/SOFTWARE".into(),
        size: bytes.len() as u64,
        encrypted: false,
        content_identity: "software".into(),
        companions: Vec::new(),
        evidence_kind: "registry_hive".into(),
        parser: "registry".into(),
        category: "Registry".into(),
        modified_at: None,
    };
    let extracted = extract_registry_candidate(&candidate, &bytes, None, None, None);
    assert_eq!(extracted.artifacts.len(), 7);
    let records = extracted
        .artifacts
        .iter()
        .map(|artifact| persistence_source::detail::artifact_record(artifact, "source-1"))
        .collect::<Vec<_>>();
    ArtifactRecordStore::from_source_store(store.clone())
        .upsert_batch(&records, PersistMode::Buffer)
        .unwrap();
    rebuild(&store, "source-1", &AtomicBool::new(false)).unwrap();
    let envelope = store
        .read_envelope(
            &store.database().read_snapshot(),
            KeyspaceName::Meta,
            &summary_key("source-1").unwrap(),
        )
        .unwrap()
        .unwrap();
    let summary: ReportSourceSummaryDto = envelope.decode_payload().unwrap();
    assert_eq!(summary.operating_system.record_count, 6);
    assert!(summary.operating_system.entities.iter().any(|entity| entity
        .fields
        .get("productName")
        .is_some_and(|value| value == "Forensics Fixture OS")));
    assert_eq!(summary.hardware.record_count, 0);
    assert_eq!(summary.browsers.record_count, 0);
    assert_eq!(summary.email.record_count, 0);
    let json = serde_json::to_value(&summary).unwrap();
    assert!(json.get("artifacts").is_none());
    assert!(json.get("timeline_events").is_none());
}

#[test]
fn cancelled_projection_does_not_publish_summary() {
    let root = tempfile::tempdir().unwrap();
    let store = SourceStore::open(root.path().join("source.fjall"), Default::default()).unwrap();
    assert!(matches!(
        rebuild(&store, "source-1", &AtomicBool::new(true)),
        Err(AnalysisServiceError::Cancelled)
    ));
    assert!(store
        .read_envelope(
            &store.database().read_snapshot(),
            KeyspaceName::Meta,
            &summary_key("source-1").unwrap()
        )
        .unwrap()
        .is_none());
}
