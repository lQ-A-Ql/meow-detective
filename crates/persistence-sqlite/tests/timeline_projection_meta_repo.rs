use persistence_sqlite::{
    open_in_memory,
    repositories::timeline_projection_repo::{TimelineProjectionMetaState, TimelineProjectionRepo},
    runner,
};

#[test]
fn current_meta_table_is_idempotent_and_deferred_is_not_done() {
    let conn = open_in_memory().expect("open case database");
    runner::run_all(&conn).expect("run migrations");
    let repo = TimelineProjectionRepo::new(&conn);
    repo.ensure_meta_table().expect("ensure table");
    repo.ensure_meta_table().expect("ensure table again");

    let columns = conn
        .prepare("SELECT name FROM pragma_table_info('timeline_projection_meta')")
        .expect("prepare column query")
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query columns")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect columns");
    for expected in ["deferred_reason", "required_bytes", "available_bytes"] {
        assert!(columns.contains(&expected.to_string()));
    }

    repo.mark_done("timeline_file_v3", 42, "identity-done")
        .expect("mark done");
    assert!(repo
        .is_done("timeline_file_v3", "identity-done")
        .expect("is done"));
    let status = repo
        .status("timeline_file_v3")
        .expect("status")
        .expect("row exists");
    assert_eq!(status.state, TimelineProjectionMetaState::Done);
    assert_eq!(status.inserted_count, 42);
    assert_eq!(status.input_identity, "identity-done");

    repo.mark_deferred(
        "timeline_file_v3",
        "identity-deferred",
        1_200_000_000,
        900_000_000,
        "TIMELINE_WORKSPACE_LIMIT",
    )
    .expect("mark deferred");
    assert!(!repo
        .is_done("timeline_file_v3", "identity-deferred")
        .expect("is done deferred"));
    assert!(!repo
        .is_done("timeline_file_v3", "identity-done")
        .expect("is done old identity"));
    let status = repo
        .status("timeline_file_v3")
        .expect("status after deferred")
        .expect("row exists");
    assert_eq!(status.state, TimelineProjectionMetaState::Deferred);
    assert_eq!(status.input_identity, "identity-deferred");
    assert_eq!(status.required_bytes, 1_200_000_000);
    assert_eq!(status.available_bytes, 900_000_000);
    assert_eq!(
        status.deferred_reason.as_deref(),
        Some("TIMELINE_WORKSPACE_LIMIT")
    );

    assert!(repo
        .status("missing-key")
        .expect("status missing")
        .is_none());
}

#[test]
fn meta_table_rejects_incomplete_layout_without_adding_columns() {
    let connection = open_in_memory().unwrap();
    connection
        .execute_batch(
            "CREATE TABLE timeline_projection_meta (
                projection_key TEXT PRIMARY KEY NOT NULL,
                status TEXT NOT NULL,
                inserted_count INTEGER NOT NULL DEFAULT 0,
                input_identity TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );",
        )
        .unwrap();
    assert!(TimelineProjectionRepo::new(&connection)
        .ensure_meta_table()
        .is_err());
    let count: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('timeline_projection_meta')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 5);
}
