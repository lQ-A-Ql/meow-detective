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
use std::sync::atomic::{AtomicBool, Ordering};

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
    persistence_source::fjall::seal_source_generation_with_control(
        &store,
        "source-1",
        "attempt-1",
        &"a".repeat(64),
        &AtomicBool::new(false),
        |_, _| {},
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

    let generation = super::begin_streaming(
        &connection,
        "source-1",
        &[],
        &AtomicBool::new(false),
        |_| {},
        |_| {},
    )
    .unwrap();
    generation
        .rebuild_derived_metrics(&AtomicBool::new(false), |_| {})
        .unwrap();

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

fn assert_no_build_generation(root: &std::path::Path) {
    let source_dir = root.join("case/sources/source-1");
    let names = std::fs::read_dir(source_dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(
        names
            .iter()
            .all(|name| !name.starts_with("source.fjall.build.")),
        "{names:?}"
    );
}

#[test]
fn cancellation_during_clone_removes_build_and_preserves_published_records() {
    let (root, connection, source) = setup();
    let published = ArtifactRecordStore::from_source_store(source.clone());
    let records = (0..5_000)
        .map(|index| {
            persistence_source::detail::artifact_record(
                &artifact(&format!("extra-{index}"), "old", "linux.web.old"),
                "source-1",
            )
        })
        .collect::<Vec<_>>();
    published
        .upsert_batch(&records, PersistMode::SyncAll)
        .unwrap();
    let revision = published
        .cursor_revision(&published.read_snapshot())
        .unwrap();
    let cancel = AtomicBool::new(false);
    let mut copied = 0;
    let result = super::begin_streaming(
        &connection,
        "source-1",
        &[],
        &cancel,
        |progress| {
            copied = progress.records_copied;
            if copied > 0 {
                cancel.store(true, Ordering::Relaxed);
            }
        },
        |_| {},
    );
    assert!(matches!(
        result,
        Err(crate::analysis_service::error::AnalysisServiceError::Cancelled)
    ));
    assert!(copied > 0);
    assert_no_build_generation(root.path());
    let snapshot = published.read_snapshot();
    assert_eq!(published.count(&snapshot, Some("source-1")).unwrap(), 5_001);
    assert_eq!(published.cursor_revision(&snapshot).unwrap(), revision);
    assert!(published
        .get(&snapshot, "source-1", "old")
        .unwrap()
        .is_some());
}

#[test]
fn dropping_streaming_generation_releases_store_before_removing_build() {
    let (root, connection, _source) = setup();
    let generation = super::begin_streaming(
        &connection,
        "source-1",
        &[],
        &AtomicBool::new(false),
        |_| {},
        |_| {},
    )
    .unwrap();
    let path = generation.staged.path.clone();
    assert!(path.is_dir());
    drop(generation);
    assert!(!path.exists());
    assert_no_build_generation(root.path());
}

#[test]
fn failed_nonstreaming_stage_removes_build_and_preserves_published_records() {
    let (root, connection, source) = setup();
    let invalid = artifact("", "old", "linux.web.new");
    assert!(super::stage(
        &connection,
        &root.path().join("case"),
        "source-1",
        &[invalid],
        &[],
        &[],
    )
    .is_err());
    assert_no_build_generation(root.path());
    let published = ArtifactRecordStore::from_source_store(source);
    let snapshot = published.read_snapshot();
    assert_eq!(published.count(&snapshot, Some("source-1")).unwrap(), 1);
    assert!(published
        .get(&snapshot, "source-1", "old")
        .unwrap()
        .is_some());
}

#[test]
fn replacement_reads_only_selected_source_objects_and_preserves_published_data() {
    let (_root, connection, source) = setup();
    let records = (0..2_000)
        .map(|index| {
            persistence_source::detail::artifact_record(
                &artifact(
                    &format!("unrelated-{index}"),
                    &format!("object-{index}"),
                    "linux.web.old",
                ),
                "source-1",
            )
        })
        .collect::<Vec<_>>();
    let published = ArtifactRecordStore::from_source_store(source.clone());
    published
        .upsert_batch(&records, PersistMode::SyncAll)
        .unwrap();
    persistence_source::fjall::instrumentation::enable();
    let generation = super::begin_streaming(
        &connection,
        "source-1",
        &[AnalysisOutputReplacement {
            source_object_id: "old".into(),
            producer: AnalysisOutputProducer::Prefix("linux.web."),
        }],
        &AtomicBool::new(false),
        |_| {},
        |processed| {
            if processed == 0 {
                persistence_source::fjall::instrumentation::reset();
            }
        },
    )
    .unwrap();
    let counters = persistence_source::fjall::instrumentation::snapshot();
    assert!(
        counters.scan_entries < 20,
        "replacement scanned unrelated objects: {counters:?}"
    );
    let staged = ArtifactRecordStore::from_source_store(generation.store.clone());
    assert!(staged
        .get(&staged.read_snapshot(), "source-1", "old")
        .unwrap()
        .is_none());
    assert!(published
        .get(&published.read_snapshot(), "source-1", "old")
        .unwrap()
        .is_some());
}

#[test]
fn cancellation_during_targeted_replacement_discards_the_new_generation() {
    let (root, connection, source) = setup();
    let cancel = AtomicBool::new(false);
    let result = super::begin_streaming(
        &connection,
        "source-1",
        &[AnalysisOutputReplacement {
            source_object_id: "old".into(),
            producer: AnalysisOutputProducer::Prefix("linux.web."),
        }],
        &cancel,
        |_| {},
        |processed| {
            if processed > 0 {
                cancel.store(true, Ordering::Release);
            }
        },
    );
    assert!(matches!(
        result,
        Err(crate::analysis_service::AnalysisServiceError::Cancelled)
    ));
    assert_no_build_generation(root.path());
    let published = ArtifactRecordStore::from_source_store(source);
    assert!(published
        .get(&published.read_snapshot(), "source-1", "old")
        .unwrap()
        .is_some());
}
