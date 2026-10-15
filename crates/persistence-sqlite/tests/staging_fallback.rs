use persistence_sqlite::repositories::staging_repo::StagingRepo;
use rusqlite::Connection;

fn staging() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!("../src/migrations/scripts/staging_001.sql"))
        .unwrap();
    conn.execute_batch("INSERT INTO file_entries (id,data_source_id,path,name,entry_type) VALUES ('old','source','/old','old','file')").unwrap();
    for (key, value) in [
        ("status", "running"),
        ("enum_strategy", "mft"),
        ("mft_records", "100"),
        ("file_count", "90"),
    ] {
        StagingRepo::set_staging_meta(&conn, key, value).unwrap();
    }
    conn
}

#[test]
fn fallback_reset_clears_rows_and_derived_markers_but_preserves_running_state() {
    let conn = staging();
    StagingRepo::reset_enum_for_fallback(&conn).unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM file_entries", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    for key in ["enum_strategy", "mft_records", "file_count"] {
        assert_eq!(StagingRepo::get_staging_meta(&conn, key).unwrap(), None);
    }
    assert_eq!(
        StagingRepo::get_staging_meta(&conn, "status")
            .unwrap()
            .as_deref(),
        Some("running")
    );
}

#[test]
fn failed_metadata_cleanup_rolls_back_the_row_deletion() {
    let conn = staging();
    conn.execute_batch("CREATE TRIGGER reject_cleanup BEFORE DELETE ON staging_meta BEGIN SELECT RAISE(ABORT, 'synthetic cleanup failure'); END").unwrap();
    assert!(StagingRepo::reset_enum_for_fallback(&conn).is_err());
    assert!(conn.is_autocommit());
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM file_entries", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        StagingRepo::get_staging_meta(&conn, "enum_strategy")
            .unwrap()
            .as_deref(),
        Some("mft")
    );
}
