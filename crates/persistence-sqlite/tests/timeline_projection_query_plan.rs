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

#[test]
fn source_schema_graph_projection_uses_covering_timeline_pages_and_target_lookup() {
    let connection = Connection::open_in_memory().unwrap();
    runner::run_source_all(&connection).unwrap();
    connection
        .execute_batch(
            "WITH RECURSIVE numbers(n) AS (
            VALUES(0) UNION ALL SELECT n+1 FROM numbers WHERE n < 19999
         ) INSERT INTO timeline_events
            (id,case_id,source_object_id,event_type,ts,title)
         SELECT printf('event:%08d',n),'case','file','FILE_MODIFIED',
                '2026-01-01T00:00:00Z','event'
         FROM numbers;
         INSERT INTO graph_nodes
            (id,case_id,node_type,label,created_at)
         VALUES ('file','case','file','file','2026-01-01T00:00:00Z');",
        )
        .unwrap();

    let plans = [
        (
            "node",
            "EXPLAIN QUERY PLAN
             SELECT id, case_id, title, event_type
             FROM timeline_events
             WHERE case_id = ?1 AND id >= ?2 AND id <= ?3",
        ),
        (
            "edge",
            "EXPLAIN QUERY PLAN
             SELECT event.id, event.case_id, event.source_object_id
             FROM timeline_events event
             WHERE event.case_id = ?1 AND event.id >= ?2 AND event.id <= ?3
               AND event.source_object_id <> ''",
        ),
        (
            "edge-target",
            "EXPLAIN QUERY PLAN
             SELECT event.id, event.case_id, event.source_object_id
             FROM timeline_events event
             WHERE event.case_id = ?1 AND event.id >= ?2 AND event.id <= ?3
               AND event.source_object_id <> ''
               AND EXISTS (
                   SELECT 1 FROM graph_nodes target
                   WHERE target.id = event.source_object_id
               )",
        ),
    ];
    for (name, sql) in plans {
        let details = connection
            .prepare(sql)
            .unwrap()
            .query_map(["case", "event:00010000", "event:00012000"], |row| {
                row.get::<_, String>(3)
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        println!("graph {name} query plan={details:?}");
        assert!(
            details
                .iter()
                .any(|detail| detail.contains("idx_source_timeline_case_id_id")),
            "{name} plan did not use the case/id range: {details:?}"
        );
        assert!(
            !details.iter().any(|detail| detail.contains("TEMP B-TREE")),
            "{name} plan introduced a temporary sort: {details:?}"
        );
        if name == "edge-target" {
            assert!(
                details
                    .iter()
                    .any(|detail| detail.contains("graph_nodes") && detail.contains("id=?")),
                "target existence lookup did not use graph_nodes primary key: {details:?}"
            );
        }
    }
}
