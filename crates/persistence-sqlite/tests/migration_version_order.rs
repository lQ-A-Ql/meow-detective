use persistence_sqlite::runner;
use rusqlite::Connection;

fn migration_log(names: &[&str]) -> Connection {
    let connection = Connection::open_in_memory().expect("open database");
    connection
        .execute_batch(
            "CREATE TABLE schema_migrations (
                 id INTEGER PRIMARY KEY,
                 name TEXT NOT NULL UNIQUE,
                 applied_at TEXT NOT NULL DEFAULT (datetime('now'))
             )",
        )
        .expect("create migration log");
    for name in names {
        connection
            .execute("INSERT INTO schema_migrations(name) VALUES (?1)", [name])
            .expect("insert migration");
    }
    connection
}

#[test]
fn source_version_uses_registry_order_after_an_older_gap_is_repaired() {
    let connection = migration_log(&[
        "source_038_linux_evidence_facts",
        "source_036_android_analysis",
    ]);

    assert_eq!(
        runner::current_version(&connection).expect("read source version"),
        Some("source_038_linux_evidence_facts".to_string())
    );
}

#[test]
fn unknown_migration_remains_visible_for_fail_closed_version_checks() {
    let connection = migration_log(&["source_038_linux_evidence_facts", "source_999_unknown"]);

    assert_eq!(
        runner::current_version(&connection).expect("read source version"),
        Some("source_999_unknown".to_string())
    );
}
