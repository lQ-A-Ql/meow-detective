use crate::repositories::source_projection_change_repo::{
    SourceProjectionChangeOperation as Operation, SourceProjectionChangeRepo,
    SourceProjectionEntity as Entity,
};

fn source_db() -> (tempfile::TempDir, rusqlite::Connection) {
    let directory = tempfile::tempdir().unwrap();
    let connection = crate::open_or_create_source(&directory.path().join("source.db")).unwrap();
    connection
        .execute(
            "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at)
             VALUES ('source-1', 'case-1', 'Source', 'e01', 'source.E01', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    (directory, connection)
}

#[test]
fn change_log_coalesces_entity_updates_and_preserves_later_revisions() {
    let (_directory, connection) = source_db();
    let repo = SourceProjectionChangeRepo::new(&connection);
    assert_eq!(repo.high_water().unwrap(), 0);

    connection
        .execute(
            "INSERT INTO artifacts (id, data_source_id, artifact_type, title, summary)
             VALUES ('artifact-1', 'source-1', 'family', 'first', 'first')",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO timeline_events (id, case_id, source_object_id, event_type, ts, title)
             VALUES ('event-1', 'case-1', 'file-1', 'TEST', '2026-01-01T00:00:00Z', 'first')",
            [],
        )
        .unwrap();
    let initial_revision = repo.high_water().unwrap();
    assert_eq!(initial_revision, 2);

    connection
        .execute(
            "UPDATE artifacts SET title = 'updated' WHERE id = 'artifact-1'",
            [],
        )
        .unwrap();
    connection
        .execute("DELETE FROM timeline_events WHERE id = 'event-1'", [])
        .unwrap();
    let high_water = repo.high_water().unwrap();
    assert_eq!(high_water, 5);

    let changes = repo.list_after(0, high_water, 10).unwrap();
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0].entity, Entity::Artifact);
    assert_eq!(changes[0].entity_id, "artifact-1");
    assert_eq!(changes[0].operation, Operation::Upsert);
    assert_eq!(changes[0].revision, 4);
    assert_eq!(changes[1].entity, Entity::Timeline);
    assert_eq!(changes[1].entity_id, "event-1");
    assert_eq!(changes[1].source_id, "source-1");
    assert_eq!(changes[1].operation, Operation::Delete);
    assert_eq!(changes[1].revision, 5);

    repo.prune_through(initial_revision).unwrap();
    assert_eq!(repo.list_after(0, high_water, 10).unwrap().len(), 2);
    repo.prune_through(high_water).unwrap();
    assert!(repo.list_after(0, high_water, 10).unwrap().is_empty());
}

#[test]
fn empty_change_page_does_not_scan_beyond_its_snapshot_high_water() {
    let (_directory, connection) = source_db();
    let repo = SourceProjectionChangeRepo::new(&connection);
    let high_water = repo.high_water().unwrap();
    assert!(repo.list_after(0, high_water, 10).unwrap().is_empty());
    assert!(repo
        .list_after(high_water, high_water, 10)
        .unwrap()
        .is_empty());
}
