use persistence_sqlite::{repositories::timeline_repo::with_deferred_empty_indexes, DbError};
use rusqlite::Connection;

fn indexes(conn: &Connection) -> Vec<(String, String)> {
    conn.prepare("SELECT name,sql FROM sqlite_master WHERE type='index' AND tbl_name='timeline_events' AND sql IS NOT NULL ORDER BY name")
        .unwrap().query_map([], |row| Ok((row.get(0)?, row.get(1)?))).unwrap().collect::<Result<_,_>>().unwrap()
}

#[test]
fn disk_full_during_projection_restores_indexes_and_allows_clean_retry() {
    let temporary = tempfile::tempdir().unwrap();
    let mut conn =
        persistence_sqlite::open_or_create_source(&temporary.path().join("source.db")).unwrap();
    let original = indexes(&conn);
    let pages: u32 = conn
        .pragma_query_value(None, "page_count", |row| row.get(0))
        .unwrap();
    conn.pragma_update(None, "max_page_count", pages + 2)
        .unwrap();
    let result = {
        let mut tx = conn.transaction().unwrap();
        with_deferred_empty_indexes(&mut tx, |conn| -> Result<(), DbError> {
            conn.execute(
                "INSERT INTO source_meta(key,value) VALUES ('test.revision','1')",
                [],
            )?;
            conn.execute_batch("WITH RECURSIVE n(v) AS (VALUES(0) UNION ALL SELECT v+1 FROM n WHERE v<9999)
                INSERT INTO timeline_events(id,case_id,source_object_id,event_type,ts,title,description)
                SELECT printf('event:%08d',v),'case','file','FILE_MODIFIED','2026-10-18T00:00:00Z','event',hex(zeroblob(4096)) FROM n;")?;
            Ok(())
        })
    };
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("full"));
    assert_eq!(indexes(&conn), original);
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM timeline_events", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM source_meta WHERE key='test.revision'",
            [],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        0
    );
    conn.pragma_update(None, "max_page_count", 1_000_000)
        .unwrap();
    let mut tx = conn.transaction().unwrap();
    with_deferred_empty_indexes(&mut tx, |conn| -> Result<(), DbError> {
        conn.execute("INSERT INTO timeline_events(id,case_id,event_type,ts,title,source_object_id) VALUES ('retry','case','FILE_MODIFIED','2026-10-18T00:00:00Z','event','file')", [])?;
        Ok(())
    }).unwrap();
    tx.commit().unwrap();
    assert_eq!(indexes(&conn), original);
    assert_eq!(
        conn.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}
