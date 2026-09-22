use persistence_sqlite::{
    open_in_memory,
    repositories::linux_import_set_repo::{
        LinuxImportSetMemberRecord, LinuxImportSetRecord, LinuxImportSetRepo,
    },
    runner,
};

#[test]
fn member_readiness_tracks_source_import_terminal_state() {
    let conn = open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO cases (id, name) VALUES ('case-set', 'set')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at)
         VALUES ('source-1', 'case-set', 'node', 'e01', 'node.E01', datetime('now'))",
        [],
    )
    .unwrap();
    let repo = LinuxImportSetRepo::new(&conn);
    repo.insert(&LinuxImportSetRecord {
        id: "set-1".to_string(),
        case_id: "case-set".to_string(),
        name: "set".to_string(),
        root_path: "root".to_string(),
        import_state: "importing".to_string(),
        member_count: 1,
        ready_count: 0,
        failed_count: 0,
        last_error: None,
    })
    .unwrap();
    repo.insert_member(&LinuxImportSetMemberRecord {
        import_set_id: "set-1".to_string(),
        member_index: 0,
        source_path: "node.E01".to_string(),
        source_kind: "e01".to_string(),
        data_source_id: Some("source-1".to_string()),
        import_state: "importing".to_string(),
        last_error: None,
    })
    .unwrap();
    repo.update_member_state_by_source("source-1", "ready_metadata", None)
        .unwrap();
    let member = repo.find_members("set-1").unwrap().pop().unwrap();
    assert_eq!(member.import_state, "ready_metadata");
}
