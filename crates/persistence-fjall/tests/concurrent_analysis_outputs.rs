use chrono::{Duration, Utc};
use persistence_fjall::{
    ArtifactRecord, ArtifactRecordStore, DatabaseConfig, PersistMode, SourceStore, TimelineRecord,
    TimelineRecordStore,
};
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn workers_write_independent_artifacts_and_timeline_before_one_metric_rebuild() {
    let root = tempfile::tempdir().unwrap();
    let source = SourceStore::open(root.path(), DatabaseConfig::default()).unwrap();
    let artifacts = ArtifactRecordStore::from_source_store(source.clone());
    let timeline = TimelineRecordStore::from_source_store(source.clone());
    std::thread::scope(|scope| {
        for worker in 0..4 {
            let artifacts = artifacts.clone();
            let timeline = timeline.clone();
            scope.spawn(move || {
                let artifact_records = (0..100)
                    .map(|item| ArtifactRecord {
                        id: format!("artifact-{worker}-{item}"),
                        data_source_id: "source".into(),
                        artifact_type: "ConcurrentArtifact".into(),
                        source_object_id: Some(format!("file-{worker}-{item}")),
                        content_sha256: None,
                        attrs: json!({"worker": worker, "item": item}),
                    })
                    .collect::<Vec<_>>();
                artifacts
                    .upsert_batch_independent(&artifact_records, PersistMode::Buffer)
                    .unwrap();
                let timeline_records = (0..100)
                    .map(|item| TimelineRecord {
                        id: format!("event-{worker}-{item}"),
                        data_source_id: "source".into(),
                        source_object_id: format!("file-{worker}-{item}"),
                        event_type: "ConcurrentEvent".into(),
                        timestamp: Utc::now() - Duration::seconds(item),
                        title: format!("event {worker}/{item}"),
                        description: String::new(),
                        parser_id: Some("concurrent.test".into()),
                        parser_version: Some("1".into()),
                        confidence: None,
                        source_attribution: None,
                        attrs: BTreeMap::new(),
                    })
                    .collect::<Vec<_>>();
                timeline
                    .upsert_batch_independent(&timeline_records, PersistMode::Buffer)
                    .unwrap();
            });
        }
    });
    artifacts
        .rebuild_metrics_for_source("source", PersistMode::Buffer)
        .unwrap();
    timeline
        .rebuild_count_indexes("source", PersistMode::Buffer)
        .unwrap();
    source.database().persist(PersistMode::SyncAll).unwrap();

    let snapshot = artifacts.read_snapshot();
    assert_eq!(artifacts.count(&snapshot, Some("source")).unwrap(), 400);
    assert_eq!(artifacts.cursor_revision(&snapshot).unwrap(), 1);
    assert_eq!(
        timeline
            .count_by_time(&snapshot, "source", None, None)
            .unwrap(),
        400
    );
    assert_eq!(timeline.cursor_revision(&snapshot).unwrap(), 1);
}
