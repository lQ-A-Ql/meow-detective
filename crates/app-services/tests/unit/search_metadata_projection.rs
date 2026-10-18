use super::MetadataProjection;
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
fn catalog_and_domain_adapters_preserve_identical_metadata() {
    let source = DataSourceId("source".into());
    let projection = MetadataProjection::new(&source, "revision-1".into());
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let actual = conn
        .query_row(
            "SELECT 'mft:1:42','[P1]/report.txt','report.txt','file',42,'txt',
        '2026-10-18T08:30:00+08:00',1,1,0,1,'source'",
            [],
            |row| projection.row(row, row.get(0)?),
        )
        .unwrap();
    assert_eq!(actual, projection.entry(&entry()).unwrap());
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
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    assert!(conn
        .query_row(
            "SELECT 'id','path','name','file',NULL,NULL,NULL,0,0,0,0,'other'",
            [],
            |row| projection.row(row, "id".into())
        )
        .is_err());
}

#[test]
fn nullable_metadata_and_partition_identity_are_retained() {
    let source = DataSourceId("source".into());
    let projection = MetadataProjection::new(&source, "revision-1".into());
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let document = conn
        .query_row(
            "SELECT 'mft:2:42','[P2]/report','report','directory',NULL,NULL,
        'invalid timestamp',0,0,0,0,'source'",
            [],
            |row| projection.row(row, row.get(0)?),
        )
        .unwrap();
    assert_eq!(document.file_id, "mft:2:42");
    assert_eq!(document.path, "[P2]/report");
    assert_eq!(document.modified_at, None);
    assert_eq!(document.size, None);
    assert!(document.extension.is_empty());
}
