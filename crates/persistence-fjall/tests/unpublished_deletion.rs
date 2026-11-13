use chrono::{DateTime, Utc};
use persistence_fjall::{
    clone_verified_generation_with_control, seal_source_generation_with_control, ArtifactRecord,
    ArtifactRecordStore, DatabaseConfig, PersistMode, SourceStore, TimelineRecord,
    TimelineRecordStore, VerifiedGenerationClone,
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{atomic::AtomicBool, Barrier},
};

const SOURCE: &str = "source";

fn artifact(id: &str) -> ArtifactRecord {
    ArtifactRecord {
        id: id.into(),
        data_source_id: SOURCE.into(),
        artifact_type: "Result".into(),
        source_object_id: Some(id.into()),
        content_sha256: None,
        attrs: json!({ "id": id }),
    }
}

fn event(id: &str) -> TimelineRecord {
    TimelineRecord {
        id: id.into(),
        data_source_id: SOURCE.into(),
        source_object_id: id.into(),
        event_type: "FILE_MODIFIED".into(),
        timestamp: DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
        title: id.into(),
        description: String::new(),
        parser_id: Some("parser".into()),
        parser_version: Some("1".into()),
        confidence: None,
        source_attribution: None,
        attrs: BTreeMap::new(),
    }
}

fn fixture() -> (
    tempfile::TempDir,
    SourceStore,
    SourceStore,
    VerifiedGenerationClone,
) {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.codex-tmp/unpublished-deletion");
    std::fs::create_dir_all(&base).unwrap();
    let root = tempfile::tempdir_in(base).unwrap();
    let original =
        SourceStore::open(root.path().join("source.fjall"), DatabaseConfig::default()).unwrap();
    ArtifactRecordStore::from_source_store(original.clone())
        .upsert_batch(&[artifact("old"), artifact("keep")], PersistMode::SyncAll)
        .unwrap();
    TimelineRecordStore::from_source_store(original.clone())
        .upsert(&event("old"), PersistMode::SyncAll)
        .unwrap();
    seal_source_generation_with_control(
        &original,
        SOURCE,
        "original",
        &"a".repeat(64),
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    let (target, generation) = clone_verified_generation_with_control(
        &original,
        SOURCE,
        root.path().join("staging"),
        DatabaseConfig::default(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    (root, original, target, generation)
}

#[test]
fn deletion_during_independent_writes_preserves_records_and_rebuilds_exact_counts() {
    let (_root, original, target, generation) = fixture();
    let artifacts = ArtifactRecordStore::from_source_store(target.clone());
    let timeline = TimelineRecordStore::from_source_store(target.clone());
    artifacts
        .upsert_batch_independent(&[artifact("partial")], PersistMode::Buffer)
        .unwrap();
    timeline
        .upsert_batch_independent(&[event("partial")], PersistMode::Buffer)
        .unwrap();
    let barrier = Barrier::new(2);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            barrier.wait();
            for index in 0..64 {
                artifacts
                    .upsert_batch_independent(
                        &[artifact(&format!("new-{index}"))],
                        PersistMode::Buffer,
                    )
                    .unwrap();
                timeline
                    .upsert_batch_independent(
                        &[event(&format!("new-{index}"))],
                        PersistMode::Buffer,
                    )
                    .unwrap();
            }
        });
        scope.spawn(|| {
            barrier.wait();
            let ids = ["old".to_string(), "partial".to_string()];
            assert_eq!(
                artifacts
                    .delete_batch_in_generation(&generation, SOURCE, &ids, PersistMode::Buffer)
                    .unwrap(),
                2
            );
            assert_eq!(
                timeline
                    .delete_batch_in_generation(&generation, SOURCE, &ids, PersistMode::Buffer)
                    .unwrap(),
                2
            );
        });
    });
    let cancel = AtomicBool::new(false);
    assert!(!artifacts
        .type_counts_ready(&artifacts.read_snapshot(), SOURCE)
        .unwrap());
    assert!(!timeline
        .counts_ready(&timeline.read_snapshot(), SOURCE)
        .unwrap());
    artifacts
        .rebuild_metrics_for_source_with_control(SOURCE, PersistMode::Buffer, &cancel, |_| {})
        .unwrap();
    timeline
        .rebuild_count_indexes_with_control(SOURCE, PersistMode::Buffer, &cancel, |_| {})
        .unwrap();
    assert_eq!(
        artifacts
            .count(&artifacts.read_snapshot(), Some(SOURCE))
            .unwrap(),
        65
    );
    assert_eq!(
        timeline
            .count_by_time(&timeline.read_snapshot(), SOURCE, None, None)
            .unwrap(),
        64
    );
    for id in ["old", "partial"] {
        assert!(artifacts
            .get(&artifacts.read_snapshot(), SOURCE, id)
            .unwrap()
            .is_none());
        assert!(timeline
            .get(&timeline.read_snapshot(), SOURCE, id)
            .unwrap()
            .is_none());
    }
    for index in 0..64 {
        let id = format!("new-{index}");
        assert!(artifacts
            .get(&artifacts.read_snapshot(), SOURCE, &id)
            .unwrap()
            .is_some());
        assert!(timeline
            .get(&timeline.read_snapshot(), SOURCE, &id)
            .unwrap()
            .is_some());
    }
    let originals = ArtifactRecordStore::from_source_store(original);
    assert_eq!(
        originals
            .count(&originals.read_snapshot(), Some(SOURCE))
            .unwrap(),
        2
    );
}

#[test]
fn staging_delete_requires_the_verified_target_and_source() {
    let (_root, original, target, generation) = fixture();
    let ids = ["old".to_string()];
    assert!(ArtifactRecordStore::from_source_store(original.clone())
        .delete_batch_in_generation(&generation, SOURCE, &ids, PersistMode::Buffer)
        .is_err());
    assert!(TimelineRecordStore::from_source_store(original)
        .delete_batch_in_generation(&generation, SOURCE, &ids, PersistMode::Buffer)
        .is_err());
    assert!(ArtifactRecordStore::from_source_store(target.clone())
        .delete_batch_in_generation(&generation, "another-source", &ids, PersistMode::Buffer)
        .is_err());
    assert!(TimelineRecordStore::from_source_store(target)
        .delete_batch_in_generation(&generation, "another-source", &ids, PersistMode::Buffer)
        .is_err());
}

#[test]
fn staging_delete_rejects_a_target_after_sealing() {
    let (_root, _original, target, generation) = fixture();
    seal_source_generation_with_control(
        &target,
        SOURCE,
        "sealed",
        &"b".repeat(64),
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    let ids = ["old".to_string()];
    assert!(ArtifactRecordStore::from_source_store(target.clone())
        .delete_batch_in_generation(&generation, SOURCE, &ids, PersistMode::Buffer)
        .is_err());
    assert!(TimelineRecordStore::from_source_store(target)
        .delete_batch_in_generation(&generation, SOURCE, &ids, PersistMode::Buffer)
        .is_err());
}
