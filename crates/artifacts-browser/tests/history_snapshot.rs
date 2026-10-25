use artifacts_browser::{
    history::{visit_history, BrowserFamily, HistoryKind, VisitError},
    sqlite::{open_sqlite_from_bytes, DatabaseSnapshot},
    BrowserError,
};
use rusqlite::Connection;
use std::sync::atomic::{AtomicBool, Ordering};

fn populate(connection: &Connection, count: i64) {
    connection.execute_batch("CREATE TABLE urls(id INTEGER PRIMARY KEY,url TEXT,title TEXT,visit_count INTEGER,last_visit_time INTEGER);
        CREATE TABLE visits(id INTEGER PRIMARY KEY,url INTEGER,visit_time INTEGER);
        INSERT INTO urls VALUES(1,'https://example.test/','Visited',2107,13300000000000001);
        INSERT INTO urls VALUES(2,'https://example.test/summary','Summary',1,13300000000000009);").unwrap();
    append(connection, 1, count);
}

fn append(connection: &Connection, start: i64, end: i64) {
    let transaction = connection.unchecked_transaction().unwrap();
    for id in start..=end {
        transaction
            .execute("INSERT INTO visits VALUES(?1,1,13300000000000001+?1)", [id])
            .unwrap();
    }
    transaction.commit().unwrap();
}

#[test]
fn visit_cursor_reads_all_pages_and_keeps_url_summaries_distinct() {
    let writer = Connection::open_in_memory().unwrap();
    populate(&writer, 2107);
    let bytes = writer
        .serialize(rusqlite::DatabaseName::Main)
        .unwrap()
        .to_vec();
    let reader = open_sqlite_from_bytes(&bytes).unwrap();
    let mut ids = Vec::new();
    let mut summaries = 0;
    visit_history(
        &reader,
        BrowserFamily::Chromium,
        &AtomicBool::new(false),
        |row| {
            if row.kind == HistoryKind::Visit {
                ids.push(row.visit_id.unwrap());
            } else {
                summaries += 1;
                assert_eq!(row.url_id, Some(2));
            }
            Ok::<(), BrowserError>(())
        },
    )
    .unwrap();
    assert_eq!(ids, (1..=2107).collect::<Vec<_>>());
    assert_eq!(summaries, 1);
    assert!(reader.execute("DELETE FROM visits", []).is_err());
    assert_eq!(
        writer
            .query_row("SELECT count(*) FROM visits", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        2107
    );
}

#[test]
fn wal_snapshot_includes_committed_tail_and_preserves_source_bytes() {
    let workspace = tempfile::tempdir().unwrap();
    let path = workspace.path().join("History");
    let writer = Connection::open(&path).unwrap();
    writer
        .execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")
        .unwrap();
    populate(&writer, 1);
    writer
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .unwrap();
    append(&writer, 2, 1107);
    let main = std::fs::read(&path).unwrap();
    let wal_path = workspace.path().join("History-wal");
    let wal = std::fs::read(&wal_path).unwrap();
    assert!(!wal.is_empty());
    let base = open_sqlite_from_bytes(&main).unwrap();
    assert_eq!(
        base.query_row("SELECT count(*) FROM visits", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    drop(base);
    let derived = workspace.path().join("derived");
    {
        let snapshot = DatabaseSnapshot::prepare(&derived, &main, Some(&wal), None).unwrap();
        assert_eq!(snapshot.input_mode(), "databaseWithWal");
        assert_eq!(
            snapshot
                .connection()
                .query_row("SELECT count(*) FROM visits", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1107
        );
        assert!(snapshot
            .connection()
            .execute("DELETE FROM visits", [])
            .is_err());
    }
    assert_eq!(std::fs::read(&path).unwrap(), main);
    assert_eq!(std::fs::read(&wal_path).unwrap(), wal);
    assert_eq!(std::fs::read_dir(derived).unwrap().count(), 0);
}

#[test]
fn cursor_cancellation_and_unknown_schema_return_explicit_errors() {
    let connection = Connection::open_in_memory().unwrap();
    populate(&connection, 2107);
    let cancel = AtomicBool::new(false);
    let mut seen = 0;
    let result = visit_history(&connection, BrowserFamily::Chromium, &cancel, |_| {
        seen += 1;
        if seen == 700 {
            cancel.store(true, Ordering::Release);
        }
        Ok::<(), BrowserError>(())
    });
    assert!(matches!(
        result,
        Err(VisitError::Parser(BrowserError::Cancelled))
    ));
    assert_eq!(seen, 700);
    let empty = Connection::open_in_memory().unwrap();
    let result = visit_history(
        &empty,
        BrowserFamily::Chromium,
        &AtomicBool::new(false),
        |_| Ok::<(), BrowserError>(()),
    );
    assert!(matches!(
        result,
        Err(VisitError::Parser(BrowserError::Unsupported(_)))
    ));
}
