use crate::analysis_service::extraction::state::{
    AnalysisOutputProducer, AnalysisOutputReplacement,
};
use chrono::Utc;
use domain::{Artifact, ArtifactId, FileEntryId, TimelineEvent, TimelineEventId};
use persistence_source::fjall::{
    ArtifactRecordStore, DatabaseConfig, PersistMode, SourceStore, TimelineRecordStore,
};
use rusqlite::Connection;
use std::collections::BTreeMap;

fn setup() -> (tempfile::TempDir, Connection, SourceStore) {
    let root = tempfile::tempdir().unwrap();
    let case_root = root.path().join("case");
    let source_dir = case_root.join("sources").join("source-1");
    std::fs::create_dir_all(&source_dir).unwrap();
    let connection =
        persistence_sqlite::open_or_create_source(&source_dir.join("source.db")).unwrap();
    persistence_source::detail::configure_source_identity(&connection, "source-1").unwrap();
    persistence_source::detail::configure_source_fjall(&connection, &source_dir.join("source.db"))
        .unwrap();
    let shared = persistence_source::fjall::open_shared_source_store(
        source_dir.join("source.fjall"),
        DatabaseConfig::default(),
    )
    .unwrap();
    let store = (*shared).clone();
    let artifacts = ArtifactRecordStore::from_source_store(store.clone());
    artifacts
        .upsert(
            &persistence_source::detail::artifact_record(
                &artifact("old", "old", "linux.web.old"),
                "source-1",
            ),
            PersistMode::SyncAll,
        )
        .unwrap();
    let timeline = TimelineRecordStore::from_source_store(store.clone());
    timeline
        .upsert(
            &persistence_source::detail::timeline_record(
                &event("old-event", "old", "linux.web.old"),
                "source-1",
            ),
            PersistMode::SyncAll,
        )
        .unwrap();
    persistence_source::fjall::seal_source_generation(
        &store,
        "source-1",
        "attempt-1",
        &"a".repeat(64),
    )
    .unwrap();
    (root, connection, store)
}

fn artifact(id: &str, object: &str, extractor: &str) -> Artifact {
    Artifact {
        id: ArtifactId(id.into()),
        family: "LinuxWebFinding".into(),
        source_object_id: Some(FileEntryId(object.into())),
        title: id.into(),
        summary: id.into(),
        extractor_id: Some(extractor.into()),
        extractor_version: Some("1".into()),
        confidence: Some(1.0),
        source_attribution: None,
        created_at: Utc::now(),
        attrs: BTreeMap::new(),
    }
}

fn event(id: &str, object: &str, parser: &str) -> TimelineEvent {
    TimelineEvent {
        id: TimelineEventId(id.into()),
        source_object_id: object.into(),
        event_type: "FILE_MODIFIED".into(),
        timestamp: Utc::now(),
        title: id.into(),
        description: id.into(),
        parser_id: Some(parser.into()),
        parser_version: Some("1".into()),
        confidence: Some(1.0),
        source_attribution: None,
        attrs: BTreeMap::new(),
    }
}

#[test]
fn analysis_generation_replacement_preserves_previous_source_until_commit() {
    let (root, connection, source) = setup();
    let replacement = AnalysisOutputReplacement {
        source_object_id: "old".into(),
        producer: AnalysisOutputProducer::Prefix("linux.web."),
    };
    let staged = super::stage(
        &connection,
        &root.path().join("case"),
        "source-1",
        &[artifact("new", "old", "linux.web.current")],
        &[event("new-event", "old", "linux.web.current")],
        &[replacement],
    )
    .unwrap();
    let staged_path = root.path().join("case/sources/source-1").join(&staged.name);
    let old = ArtifactRecordStore::from_source_store(source.clone());
    assert!(old
        .get(&old.read_snapshot(), "source-1", "old")
        .unwrap()
        .is_some());
    let staged_store = SourceStore::open(
        root.path().join("case/sources/source-1").join(&staged.name),
        DatabaseConfig::default(),
    )
    .unwrap();
    let staged_artifacts = ArtifactRecordStore::from_source_store(staged_store.clone());
    assert!(staged_artifacts
        .get(&staged_artifacts.read_snapshot(), "source-1", "old")
        .unwrap()
        .is_none());
    assert!(staged_artifacts
        .get(&staged_artifacts.read_snapshot(), "source-1", "new")
        .unwrap()
        .is_some());
    drop(staged_store);
    drop(staged);
    assert!(!staged_path.exists());
}

#[test]
fn rebuild_derived_metrics_without_writes_leaves_derived_counts_unchanged() {
    let (_root, connection, source) = setup();
    let snapshot = source.database().read_snapshot();
    let published_artifacts = ArtifactRecordStore::from_source_store(source.clone())
        .count(&snapshot, Some("source-1"))
        .unwrap();
    let published_events = TimelineRecordStore::from_source_store(source.clone())
        .count_by_time(&snapshot, "source-1", None, None)
        .unwrap();
    assert_eq!((published_artifacts, published_events), (1, 1));
    drop(snapshot);

    let generation = super::begin_streaming(&connection, "source-1", &[]).unwrap();
    generation.rebuild_derived_metrics().unwrap();

    let snapshot = generation.store.database().read_snapshot();
    let artifacts = ArtifactRecordStore::from_source_store(generation.store.clone());
    let timeline = TimelineRecordStore::from_source_store(generation.store.clone());
    assert_eq!(
        artifacts.count(&snapshot, Some("source-1")).unwrap(),
        published_artifacts
    );
    assert_eq!(
        timeline
            .count_by_time(&snapshot, "source-1", None, None)
            .unwrap(),
        published_events
    );
    assert!(artifacts
        .get(&snapshot, "source-1", "old")
        .unwrap()
        .is_some());
    assert!(timeline
        .get(&snapshot, "source-1", "old-event")
        .unwrap()
        .is_some());
    drop(snapshot);
    drop(artifacts);
    drop(timeline);
    let super::StreamingGeneration { staged, store, .. } = generation;
    drop(store);
    drop(staged);
}

fn remove_artifact_type_count_marker(store: &SourceStore) {
    let key = persistence_source::fjall::encode_key(
        "artifact.type-count-schema",
        &[persistence_source::fjall::KeyPart::Text("source-1".into())],
    )
    .unwrap();
    store
        .database()
        .write_batch(
            &[persistence_source::fjall::WriteAction::Delete {
                keyspace: persistence_source::fjall::KeyspaceName::Meta,
                key,
            }],
            PersistMode::SyncAll,
        )
        .unwrap();
}

#[test]
fn reprocessing_without_new_outputs_prepares_legacy_artifact_type_counts() {
    let (_root, connection, source) = setup();
    remove_artifact_type_count_marker(&source);
    let published = ArtifactRecordStore::from_source_store(source);
    assert!(!published
        .type_counts_ready(&published.read_snapshot(), "source-1")
        .unwrap());

    let generation = super::begin_streaming(&connection, "source-1", &[]).unwrap();
    generation.rebuild_derived_metrics().unwrap();
    let artifacts = ArtifactRecordStore::from_source_store(generation.store.clone());
    let snapshot = artifacts.read_snapshot();
    assert!(artifacts.type_counts_ready(&snapshot, "source-1").unwrap());
    assert_eq!(
        artifacts.type_counts(&snapshot, "source-1").unwrap(),
        vec![("LinuxWebFinding".to_owned(), 1)]
    );
    assert!(!published
        .type_counts_ready(&published.read_snapshot(), "source-1")
        .unwrap());
    drop(snapshot);
    drop(artifacts);
    let super::StreamingGeneration { staged, store, .. } = generation;
    drop(store);
    drop(staged);
}

#[test]
fn reprocessing_empty_source_prepares_empty_artifact_type_counts() {
    let (_root, connection, source) = setup();
    let published = ArtifactRecordStore::from_source_store(source.clone());
    published
        .delete("source-1", "old", PersistMode::SyncAll)
        .unwrap();
    remove_artifact_type_count_marker(&source);
    let generation = super::begin_streaming(&connection, "source-1", &[]).unwrap();
    generation.rebuild_derived_metrics().unwrap();
    let artifacts = ArtifactRecordStore::from_source_store(generation.store.clone());
    let snapshot = artifacts.read_snapshot();
    assert!(artifacts.type_counts_ready(&snapshot, "source-1").unwrap());
    assert!(artifacts
        .type_counts(&snapshot, "source-1")
        .unwrap()
        .is_empty());
    assert!(!published
        .type_counts_ready(&published.read_snapshot(), "source-1")
        .unwrap());
    drop(snapshot);
    drop(artifacts);
    let super::StreamingGeneration { staged, store, .. } = generation;
    drop(store);
    drop(staged);
}
