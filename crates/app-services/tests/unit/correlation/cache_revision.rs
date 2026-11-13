use super::*;

fn cached_key(conn: &Connection) -> String {
    conn.query_row(
        "SELECT artifact_hash FROM correlation_snapshots WHERE case_id = 'case-1'",
        [],
        |row| row.get(0),
    )
    .unwrap()
}

#[test]
fn cache_refreshes_when_catalog_or_timeline_changes() {
    let conn = setup_case_db();
    insert_file(&conn, "file-1", "C:/old.txt", false);
    insert_artifact(&conn, "artifact-1", "LNK", Some("file-1"), BTreeMap::new());
    get_correlation_snapshot(&conn).unwrap();
    let first = cached_key(&conn);
    insert_file(&conn, "file-1", "C:/new.txt", false);
    get_correlation_snapshot_incremental(&conn).unwrap();
    let second = cached_key(&conn);
    assert_ne!(first, second);
    let source = persistence_source::detail::require_source_store(&conn).unwrap();
    persistence_source::fjall::TimelineRecordStore::from_source_store((*source).clone())
        .upsert_batch(
            &[persistence_source::fjall::TimelineRecord {
                id: "timeline-1".into(),
                data_source_id: "ds-1".into(),
                source_object_id: "file-1".into(),
                event_type: "FILE_MODIFIED".into(),
                timestamp: Utc::now(),
                title: "opened".into(),
                description: String::new(),
                parser_id: None,
                parser_version: None,
                confidence: None,
                source_attribution: None,
                attrs: BTreeMap::new(),
            }],
            persistence_source::fjall::PersistMode::Buffer,
        )
        .unwrap();
    get_correlation_snapshot_incremental(&conn).unwrap();
    assert_ne!(second, cached_key(&conn));
}

#[test]
fn incremental_refresh_removes_deleted_artifact_nodes() {
    let conn = setup_case_db();
    insert_file(&conn, "file-1", "C:/file.txt", false);
    insert_artifact(&conn, "artifact-1", "LNK", Some("file-1"), BTreeMap::new());
    insert_artifact(&conn, "artifact-2", "LNK", Some("file-1"), BTreeMap::new());
    get_correlation_snapshot(&conn).unwrap();
    let source = persistence_source::detail::require_source_store(&conn).unwrap();
    persistence_source::fjall::ArtifactRecordStore::from_source_store((*source).clone())
        .delete(
            "ds-1",
            "artifact-1",
            persistence_source::fjall::PersistMode::Buffer,
        )
        .unwrap();
    let refreshed = get_correlation_snapshot_incremental(&conn).unwrap();
    assert!(!refreshed
        .nodes
        .iter()
        .any(|node| node.id == "artifact:artifact-1"));
    assert!(refreshed
        .nodes
        .iter()
        .any(|node| node.id == "artifact:artifact-2"));
}
