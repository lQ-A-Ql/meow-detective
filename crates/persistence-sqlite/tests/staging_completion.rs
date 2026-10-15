use persistence_sqlite::repositories::staging_repo::StagingRepo;
use rusqlite::Connection;

#[test]
fn completion_failure_does_not_publish_done_or_partial_counters() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!("../src/migrations/scripts/staging_001.sql"))
        .unwrap();
    StagingRepo::set_staging_meta(&conn, "status", "running").unwrap();
    conn.execute_batch("CREATE TRIGGER reject_done BEFORE INSERT ON staging_meta WHEN NEW.key = 'status' AND NEW.value = 'done' BEGIN SELECT RAISE(ABORT, 'synthetic completion failure'); END").unwrap();
    assert!(StagingRepo::complete_enum(&conn, 5, 2).is_err());
    assert!(conn.is_autocommit());
    assert_eq!(
        StagingRepo::get_staging_meta(&conn, "status")
            .unwrap()
            .as_deref(),
        Some("running")
    );
    assert_eq!(
        StagingRepo::get_staging_meta(&conn, "file_count").unwrap(),
        None
    );
    conn.execute_batch("DROP TRIGGER reject_done").unwrap();
    StagingRepo::complete_enum(&conn, 5, 2).unwrap();
    assert_eq!(
        StagingRepo::get_staging_meta(&conn, "status")
            .unwrap()
            .as_deref(),
        Some("done")
    );
    assert_eq!(
        StagingRepo::get_staging_meta(&conn, "file_count")
            .unwrap()
            .as_deref(),
        Some("5")
    );
    assert_eq!(
        StagingRepo::get_staging_meta(&conn, "dir_count")
            .unwrap()
            .as_deref(),
        Some("2")
    );
}
