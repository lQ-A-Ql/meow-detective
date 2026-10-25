use super::sqlite::with_database;
use super::ExtractionOutcome;

fn database() -> Vec<u8> {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "CREATE TABLE records(id INTEGER PRIMARY KEY); INSERT INTO records VALUES(1);",
        )
        .unwrap();
    connection
        .serialize(rusqlite::DatabaseName::Main)
        .unwrap()
        .to_vec()
}

#[test]
fn browser_database_uses_real_sqlite_and_rejects_writes() {
    let bytes = database();
    let result = with_database(&bytes, "browser", |connection| {
        let count: i64 =
            connection.query_row("SELECT count(*) FROM records", [], |row| row.get(0))?;
        assert_eq!(count, 1);
        assert!(connection
            .execute("INSERT INTO records VALUES(2)", [])
            .is_err());
        Ok(ExtractionOutcome::default())
    });
    assert!(result.is_ok());
}

#[test]
fn browser_database_rejects_invalid_bytes_and_propagates_callback_errors() {
    assert!(with_database(b"invalid", "browser", |_| Ok(ExtractionOutcome::default())).is_err());
    let error = with_database(&database(), "browser", |_| {
        Err(crate::analysis_service::AnalysisServiceError::InvalidInput(
            "callback failure".into(),
        ))
    })
    .err()
    .unwrap();
    assert!(matches!(
        error,
        crate::analysis_service::AnalysisServiceError::InvalidInput(_)
    ));
}
