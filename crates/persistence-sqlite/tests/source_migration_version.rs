use persistence_sqlite::runner;

#[test]
fn source_version_is_monotonic_for_active_migrations() {
    assert!(runner::source_version_is_at_least(
        "source_038_linux_evidence_facts",
        "source_037_forensic_fingerprints"
    ));
    assert!(!runner::source_version_is_at_least(
        "source_037_forensic_fingerprints",
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
    for table in ["file_entries", "artifacts", "timeline_events"] {
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
