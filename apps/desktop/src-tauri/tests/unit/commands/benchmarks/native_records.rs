use chrono::{TimeZone, Utc};
use domain::{Artifact, ArtifactId, FileEntry};
use persistence_source::{
    fjall::{PersistMode, TimelineRecord, TimelineRecordStore},
    repositories::artifact_repo::ArtifactRepo,
};

use super::native_fixture::NativeFixture;

pub(super) const TIMELINE_TYPE: &str = "BENCH_TIMELINE";
pub(super) const ARTIFACT_TYPE: &str = "BENCH_ARTIFACT";

pub(super) fn seed_timeline(fixture: &NativeFixture, count: usize) {
    let handle = app_services::source_db::open_registered_fjall_source_read_write(
        &fixture.active.case_root,
        &fixture.sources[0],
    )
    .unwrap();
    let store = TimelineRecordStore::from_source_store(handle.source_store().clone());
    let base = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    for start in (0..count).step_by(1024) {
        let records = (start..(start + 1024).min(count))
            .map(|index| TimelineRecord {
                id: format!("benchmark-event-{index:09}"),
                data_source_id: fixture.sources[0].0.clone(),
                source_object_id: format!("benchmark-file-{index:09}"),
                event_type: TIMELINE_TYPE.to_owned(),
                timestamp: base + chrono::Duration::seconds(index as i64 * 60),
                title: format!("event {index}"),
                description: String::new(),
                parser_id: Some("benchmark.fixture".to_owned()),
                parser_version: Some("1".to_owned()),
                confidence: Some(1.0),
                source_attribution: Some("benchmark fixture".to_owned()),
                attrs: Default::default(),
            })
            .collect::<Vec<_>>();
        store.upsert_batch(&records, PersistMode::Buffer).unwrap();
    }
    handle.persist(PersistMode::SyncAll).unwrap();
}

pub(super) fn seed_artifacts(fixture: &NativeFixture, count: usize, paths: Option<&[FileEntry]>) {
    let connection = fixture.source_connection(0);
    let base = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    for start in (0..count).step_by(1024) {
        let rows = (start..(start + 1024).min(count))
            .map(|index| {
                let mut attrs = std::collections::BTreeMap::new();
                let file = paths.map(|files| &files[index % files.len()]);
                if let Some(file) = file {
                    attrs.insert("target_path".to_owned(), serde_json::json!(file.path));
                }
                Artifact {
                    id: ArtifactId(format!("benchmark-artifact-{index:09}")),
                    family: if paths.is_some() {
                        "LNK".to_owned()
                    } else {
                        ARTIFACT_TYPE.to_owned()
                    },
                    title: format!("artifact {index}"),
                    summary: String::new(),
                    source_object_id: file.map(|file| file.id.clone()),
                    extractor_id: Some("benchmark.fixture".to_owned()),
                    extractor_version: Some("1".to_owned()),
                    confidence: Some(1.0),
                    source_attribution: Some("benchmark fixture".to_owned()),
                    created_at: base + chrono::Duration::seconds(index as i64),
                    attrs,
                }
            })
            .collect::<Vec<_>>();
        ArtifactRepo::new(&connection)
            .insert_batch(&rows, &fixture.active.meta.id.0, &fixture.sources[0].0)
            .unwrap();
    }
}
