use persistence_sqlite::{
    open_in_memory,
    repositories::timeline_projection_repo::{TimelineProjectionMetaState, TimelineProjectionRepo},
    runner,
};

#[test]
fn meta_table_upgrades_idempotently_and_deferred_is_not_done() {
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
