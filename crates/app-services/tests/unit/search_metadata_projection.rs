use super::{MetadataProjection, SearchProjectionMarker};
use domain::{DataSourceId, FileEntry};

fn entry() -> FileEntry {
    serde_json::from_value(serde_json::json!({
        "id":"mft:1:42", "data_source_id":"source", "path":"[P1]/report.txt", "name":"report.txt",
        "entry_type":"File", "size":42, "ext":"txt", "deleted":true, "hidden":true,
        "system":false, "encrypted":true, "modified_at":"2026-10-18T08:30:00+08:00"
    }))
    .unwrap()
}

#[test]
fn native_catalog_adapter_preserves_metadata() {
    let source = DataSourceId("source".into());
    let projection = MetadataProjection::new(&source, "revision-1".into());
    let actual = projection.entry(&entry()).unwrap();
    assert_eq!(actual.file_id, "mft:1:42");
    assert!(actual.deleted && actual.encrypted && actual.hidden);
}

#[test]
fn foreign_source_and_replaced_revision_are_rejected() {
    let source = DataSourceId("source".into());
    let projection = MetadataProjection::new(&source, "revision-1".into());
    assert!(projection.validate(&source, "revision-2").is_err());
    assert!(projection.validate(&source, "").is_err());
    let mut wrong = entry();
    wrong.data_source_id = DataSourceId("other".into());
    assert!(projection.entry(&wrong).is_err());
}

#[test]
fn nullable_metadata_and_partition_identity_are_retained() {
    let source = DataSourceId("source".into());
    let projection = MetadataProjection::new(&source, "revision-1".into());
    let mut entry = entry();
    entry.id.0 = "mft:2:42".into();
    entry.path = "[P2]/report".into();
    entry.name = "report".into();
    entry.entry_type = domain::EntryType::Directory;
    entry.size = None;
    entry.ext = None;
    entry.modified_at = None;
    let document = projection.entry(&entry).unwrap();
    assert_eq!(document.file_id, "mft:2:42");
    assert_eq!(document.path, "[P2]/report");
    assert_eq!(document.modified_at, None);
    assert_eq!(document.size, None);
    assert!(document.extension.is_empty());
}

#[test]
fn search_projection_marker_round_trips_generation_and_revision() {
    let marker = SearchProjectionMarker {
        source_id: "source".into(),
        input_revision: "17".into(),
        generation: "generation-1".into(),
        schema_version: 5,
        index_path: "sources/source/index".into(),
    };
    let bytes = serde_json::to_vec(&marker).unwrap();
    let decoded: SearchProjectionMarker = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded, marker);
}
