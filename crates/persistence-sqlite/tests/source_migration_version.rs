use persistence_sqlite::runner;

#[test]
fn source_version_is_monotonic_for_active_migrations() {
    assert!(runner::source_version_is_at_least(
        "source_038_linux_evidence_facts",
        "source_036_android_analysis"
    ));
    assert!(!runner::source_version_is_at_least(
        "source_036_android_analysis",
        "source_038_linux_evidence_facts"
    ));
    assert!(!runner::source_version_is_at_least(
        "source_999_unknown",
        "source_038_linux_evidence_facts"
    ));
}

#[test]
fn fresh_source_schema_contains_control_tables_without_detail_tables() {
    let connection = persistence_sqlite::open_in_memory().unwrap();
    assert!(runner::run_source_all(&connection).unwrap() > 0);
    for table in [
        "file_entries",
        "artifacts",
        "timeline_events",
        "forensic_fingerprints",
        "android_system_facts",
        "android_packages",
    ] {
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "source SQLite must not create {table}");
    }
    for table in ["source_meta", "data_sources", "graph_nodes", "graph_edges"] {
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "source control table {table} must exist");
    }
}

#[test]
fn source_with_sqlite_details_is_rejected_without_migration_or_data_changes() {
    let conn = persistence_sqlite::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE file_entries(id TEXT PRIMARY KEY); INSERT INTO file_entries VALUES ('evidence')").unwrap();
    assert!(runner::run_source_all(&conn).is_err());
    let value: String = conn
        .query_row("SELECT id FROM file_entries", [], |row| row.get(0))
        .unwrap();
    assert_eq!(value, "evidence");
    let migrations: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='schema_migrations'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(migrations, 0);
    assert_eq!(
        runner::latest_source_version(),
        "source_042_android_fjall_only"
    );
}
