use persistence_sqlite::runner;
use rusqlite::Connection;

#[test]
fn current_source_schema_bounds_timeline_graph_ranges_without_temporary_sort() {
    let connection = Connection::open_in_memory().unwrap();
    runner::run_source_all(&connection).unwrap();
    connection
        .execute_batch(
            "WITH RECURSIVE numbers(n) AS (
            VALUES(0) UNION ALL SELECT n+1 FROM numbers WHERE n < 29999
         ) INSERT INTO timeline_events(id,case_id,source_object_id,event_type,ts,title)
         SELECT printf('event:%08d',n),'case','file','FILE_MODIFIED','2026-01-01T00:00:00Z','event'
         FROM numbers;",
        )
        .unwrap();
    let version = runner::current_version(&connection).unwrap();
    let mut query = connection
        .prepare(
            "EXPLAIN QUERY PLAN SELECT id FROM timeline_events
         WHERE case_id=?1 AND id>=?2 AND id<=?3 ORDER BY id",
        )
        .unwrap();
    let details = query
        .query_map(["case", "event:00010000", "event:00012000"], |row| {
            row.get::<_, String>(3)
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    println!(
        "SQLite {} current schema query plan={details:?}",
        rusqlite::version()
    );
    assert!(
        details
            .iter()
            .any(|detail| detail.contains("id>?") && detail.contains("id<?")),
        "{details:?}"
    );
    assert!(
        !details.iter().any(|detail| detail.contains("TEMP B-TREE")),
        "{details:?}"
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM timeline_events", [], |row| row
                .get::<_, u64>(0))
            .unwrap(),
        30000
    );
    assert_eq!(runner::current_version(&connection).unwrap(), version);
}
