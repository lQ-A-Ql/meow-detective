use persistence_sqlite::{
    repositories::timeline_repo::with_deferred_empty_indexes, runner, DbError,
};
use rusqlite::{Connection, OptionalExtension};

fn connection() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    runner::run_source_all(&conn).unwrap();
    conn
}

fn indexes(conn: &Connection) -> Vec<(String, String)> {
    conn.prepare("SELECT name,sql FROM sqlite_master WHERE type='index' AND tbl_name='timeline_events' AND sql IS NOT NULL ORDER BY name")
        .unwrap().query_map([], |row| Ok((row.get(0)?,row.get(1)?))).unwrap()
        .collect::<Result<_,_>>().unwrap()
}

fn insert(conn: &Connection, id: &str, attrs: &str) -> Result<usize, rusqlite::Error> {
    conn.execute(
        "INSERT INTO timeline_events(id,case_id,source_object_id,event_type,ts,title,attrs)
         VALUES (?1,'case','file','FILE_MODIFIED','2026-10-16T00:00:00Z',?1,?2)",
        [id, attrs],
    )
}

#[test]
fn empty_projection_rebuilds_exact_indexes_and_preserves_primary_and_unique_constraints() {
    let mut conn = connection();
    conn.execute_batch("CREATE UNIQUE INDEX custom_unique ON timeline_events(title)")
        .unwrap();
    let original = indexes(&conn);
    let version = runner::current_version(&conn).unwrap();
    let mut transaction = conn.transaction().unwrap();
    let (rows, _) = with_deferred_empty_indexes(&mut transaction, |conn| -> Result<_, DbError> {
        assert_eq!(indexes(conn).iter().map(|i| i.0.as_str()).collect::<Vec<_>>(), ["custom_unique"]);
        insert(conn, "event", "{}")?;
        assert!(insert(conn, "event", "{}").is_err(), "primary key stays enforced");
        assert!(conn.execute("INSERT INTO timeline_events SELECT 'other',case_id,source_object_id,event_type,ts,title,description,parser_id,parser_version,confidence,source_attribution,attrs FROM timeline_events", []).is_err(), "unique index stays enforced");
        Ok(1)
    }).unwrap();
    assert_eq!(rows, 1);
    assert_eq!(indexes(&transaction), original);
    transaction.commit().unwrap();
    assert_eq!(indexes(&conn), original);
    assert_eq!(runner::current_version(&conn).unwrap(), version);
    assert_eq!(
        conn.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}

#[test]
fn populated_projection_keeps_all_indexes_live_even_when_replacing_every_row() {
    let mut conn = connection();
    insert(&conn, "old", "{}").unwrap();
    let original = indexes(&conn);
    let mut transaction = conn.transaction().unwrap();
    with_deferred_empty_indexes(&mut transaction, |conn| -> Result<_, DbError> {
        conn.execute("DELETE FROM timeline_events", [])?;
        assert_eq!(indexes(conn), original);
        insert(conn, "new", "{}")?;
        Ok(())
    })
    .unwrap();
    transaction.commit().unwrap();
    assert_eq!(indexes(&conn), original);
}

#[test]
fn operation_error_restores_schema_rows_and_revision_even_if_outer_transaction_commits() {
    let mut conn = connection();
    let original = indexes(&conn);
    let mut transaction = conn.transaction().unwrap();
    let result = with_deferred_empty_indexes(&mut transaction, |conn| -> Result<(), DbError> {
        insert(conn, "event", "{}")?;
        conn.execute(
            "INSERT INTO source_meta(key,value) VALUES ('test.revision','1')",
            [],
        )?;
        Err(DbError::System("cancelled".into()))
    });
    assert!(result.is_err());
    transaction.commit().unwrap();
    assert_eq!(indexes(&conn), original);
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM timeline_events", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(conn
        .query_row(
            "SELECT value FROM source_meta WHERE key='test.revision'",
            [],
            |r| r.get::<_, String>(0)
        )
        .optional()
        .unwrap()
        .is_none());
}

#[test]
fn index_rebuild_failure_restores_the_complete_schema_and_discards_new_events() {
    let mut conn = connection();
    conn.execute_batch("CREATE INDEX custom_json ON timeline_events(json_extract(attrs,'$.key'))")
        .unwrap();
    let original = indexes(&conn);
    let mut transaction = conn.transaction().unwrap();
    let result = with_deferred_empty_indexes(&mut transaction, |conn| -> Result<_, DbError> {
        insert(conn, "event", "invalid json")?;
        Ok(())
    });
    assert!(result.is_err(), "index creation must fail on invalid JSON");
    transaction.commit().unwrap();
    assert_eq!(indexes(&conn), original);
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM timeline_events", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}

#[test]
fn outer_rollback_restores_indexes_after_a_successful_projection() {
    let mut conn = connection();
    let original = indexes(&conn);
    {
        let mut transaction = conn.transaction().unwrap();
        with_deferred_empty_indexes(&mut transaction, |conn| -> Result<_, DbError> {
            insert(conn, "event", "{}")?;
            Ok(())
        })
        .unwrap();
    }
    assert_eq!(indexes(&conn), original);
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM timeline_events", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn wal_reader_never_observes_unindexed_or_uncommitted_events() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("source.db");
    let mut writer = persistence_sqlite::open_or_create_source(&path).unwrap();
    let reader = persistence_sqlite::open_existing_source_read_only(&path).unwrap();
    let original = indexes(&reader);
    let mut transaction = writer.transaction().unwrap();
    with_deferred_empty_indexes(&mut transaction, |conn| -> Result<_, DbError> {
        assert!(indexes(conn).is_empty());
        insert(conn, "event", "{}")?;
        assert_eq!(indexes(&reader), original);
        assert_eq!(
            reader.query_row("SELECT COUNT(*) FROM timeline_events", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        Ok(())
    })
    .unwrap();
    transaction.commit().unwrap();
    assert_eq!(indexes(&reader), original);
    assert_eq!(
        reader
            .query_row("SELECT COUNT(*) FROM timeline_events", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn terminated_writer_recovers_the_original_indexes_without_a_manifest() {
    use std::{
        process::Command,
        thread,
        time::{Duration, Instant},
    };
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("source.db");
    let marker = temporary.path().join("uncommitted");
    let connection = persistence_sqlite::open_or_create_source(&path).unwrap();
    let original = indexes(&connection);
    drop(connection);
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "interrupted_projection_child",
            "--exact",
            "--ignored",
            "--test-threads=1",
        ])
        .env("MEOW_TIMELINE_TEST_DB", &path)
        .env("MEOW_TIMELINE_TEST_MARKER", &marker)
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while !marker.exists() && Instant::now() < deadline {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let ready = marker.exists();
    // Only terminate the helper this test spawned, including on timeout.
    let _ = child.kill();
    child.wait().unwrap();
    assert!(
        ready,
        "child did not reach the uncommitted index-deferral window"
    );
    let recovered = persistence_sqlite::open_existing_source(&path).unwrap();
    assert_eq!(indexes(&recovered), original);
    assert_eq!(
        recovered
            .query_row("SELECT COUNT(*) FROM timeline_events", [], |row| row
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        recovered
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}

#[test]
#[ignore = "helper process for terminated_writer_recovers_the_original_indexes_without_a_manifest"]
fn interrupted_projection_child() {
    use std::{thread, time::Duration};
    let path = std::env::var_os("MEOW_TIMELINE_TEST_DB").expect("test database path");
    let marker = std::env::var_os("MEOW_TIMELINE_TEST_MARKER").expect("test marker path");
    let mut conn = persistence_sqlite::open_existing_source(std::path::Path::new(&path)).unwrap();
    let mut transaction = conn.transaction().unwrap();
    let _: Result<((), Duration), DbError> =
        with_deferred_empty_indexes(&mut transaction, |conn| {
            assert!(indexes(conn).is_empty());
            insert(conn, "event", "{}")?;
            // Spill uncommitted DDL and event pages into WAL before forced termination.
            conn.cache_flush()?;
            std::fs::write(marker, b"ready")?;
            loop {
                thread::sleep(Duration::from_millis(100));
            }
        });
}
