use super::super::projection_graph::{
    load_first_graph_rows, load_graph_rows_after,
    populate_timeline_event_graph_with_batch as populate_timeline_event_graph_with_batch_inner,
    FIRST_GRAPH_PAGE_SQL, NEXT_GRAPH_PAGE_SQL,
};
use super::*;

fn populate_timeline_event_graph_with_batch(
    conn: &Connection,
    batch_size: u32,
) -> Result<Vec<String>, TimelineServiceError> {
    let cancel_token = AtomicBool::new(false);
    populate_timeline_event_graph_with_batch_inner(conn, batch_size, &cancel_token)
}

fn projection_connection() -> Connection {
    let conn = persistence_sqlite::connection::open_in_memory().expect("open in-memory database");
    conn.execute_batch(
        "CREATE TABLE data_sources (
            id TEXT PRIMARY KEY NOT NULL,
            case_id TEXT NOT NULL
         );
         CREATE TABLE file_entries (
            id TEXT PRIMARY KEY NOT NULL,
            data_source_id TEXT NOT NULL,
            path TEXT NOT NULL,
            name TEXT NOT NULL,
            entry_type TEXT NOT NULL,
            deleted INTEGER NOT NULL DEFAULT 0,
            system INTEGER NOT NULL DEFAULT 0,
            read_only INTEGER NOT NULL DEFAULT 0,
            created_at TEXT,
            modified_at TEXT,
            accessed_at TEXT,
            changed_at TEXT
         );
         CREATE TABLE timeline_events (
            id TEXT PRIMARY KEY NOT NULL,
            case_id TEXT NOT NULL,
            source_object_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            ts TEXT NOT NULL,
            title TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            parser_id TEXT,
            parser_version TEXT,
            confidence REAL,
            source_attribution TEXT,
            attrs TEXT NOT NULL DEFAULT '{}'
         );
         CREATE TABLE graph_nodes (
            id TEXT PRIMARY KEY NOT NULL,
            case_id TEXT NOT NULL,
            node_type TEXT NOT NULL,
            label TEXT NOT NULL,
            summary TEXT NOT NULL DEFAULT '',
            tags TEXT NOT NULL DEFAULT '[]',
            created_at TEXT NOT NULL
         );
         CREATE TABLE graph_edges (
            id TEXT PRIMARY KEY NOT NULL,
            case_id TEXT NOT NULL,
            source_id TEXT NOT NULL,
            target_id TEXT NOT NULL,
            edge_type TEXT NOT NULL,
            confidence REAL,
            provenance TEXT,
            created_at TEXT NOT NULL
         );
         INSERT INTO data_sources (id, case_id) VALUES ('ds-1', 'case-1');",
    )
    .expect("create projection schema");
    conn
}

fn insert_timeline_row(conn: &Connection, id: &str, source_object_id: &str) {
    conn.execute(
        "INSERT INTO timeline_events
         (id, case_id, source_object_id, event_type, ts, title, description, confidence)
         VALUES (?1, 'case-1', ?2, 'FILE_CREATED', '2026-07-18T00:00:00Z', ?1, ?1, 0.75)",
        params![id, source_object_id],
    )
    .expect("insert timeline row");
}

#[test]
fn timeline_graph_keyset_crosses_batch_boundaries_without_gaps_or_duplicates() {
    let conn = projection_connection();
    for (id, source_id) in [
        ("event-e", "file-e"),
        ("event-b", "file-b"),
        ("event-a", "file-a"),
        ("event-d", "file-d"),
        ("event-c", ""),
    ] {
        insert_timeline_row(&conn, id, source_id);
    }

    let warnings =
        populate_timeline_event_graph_with_batch(&conn, 2).expect("project timeline graph");
    let node_ids = conn
        .prepare("SELECT id FROM graph_nodes ORDER BY id")
        .expect("prepare node query")
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query graph nodes")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect graph nodes");

    assert_eq!(
        node_ids,
        vec!["event-a", "event-b", "event-c", "event-d", "event-e"]
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM graph_edges", [], |row| {
            row.get::<_, u64>(0)
        })
        .expect("count graph edges"),
        4
    );
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("1 timeline event(s) skipped"));
}

#[test]
fn timeline_graph_keyset_uses_strict_cursor_boundary() {
    let conn = projection_connection();
    for id in ["event-a", "event-b", "event-c"] {
        insert_timeline_row(&conn, id, "file");
    }
    let mut first = conn
        .prepare(FIRST_GRAPH_PAGE_SQL)
        .expect("prepare first page");
    let mut next = conn
        .prepare(NEXT_GRAPH_PAGE_SQL)
        .expect("prepare next page");

    let first_page = load_first_graph_rows(&mut first, "case-1", 2).expect("load first page");
    let second_page =
        load_graph_rows_after(&mut next, "case-1", &first_page[1].id, 2).expect("load next page");
    let final_page =
        load_graph_rows_after(&mut next, "case-1", &second_page[0].id, 2).expect("load final page");

    assert_eq!(
        first_page
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        vec!["event-a", "event-b"]
    );
    assert_eq!(second_page[0].id, "event-c");
    assert!(final_page.is_empty());
}

#[test]
fn linux_projection_excludes_only_read_only_regular_files() {
    let conn = projection_connection();
    conn.execute_batch(
        "INSERT INTO file_entries
         (id, data_source_id, path, name, entry_type, system, read_only, modified_at)
         VALUES
         ('writable-system-path', 'ds-1', '/etc/ssh/sshd_config', 'sshd_config', 'file', 1, 0, '2026-07-18T00:00:00Z'),
         ('read-only-user-path', 'ds-1', '/home/user/notes.txt', 'notes.txt', 'file', 0, 1, '2026-07-18T00:00:00Z'),
         ('directory', 'ds-1', '/var/log', 'log', 'directory', 0, 0, '2026-07-18T00:00:00Z');",
    )
    .expect("insert Linux file policy fixtures");

    let projection = materialize_file_activity_with_identity(
        &conn,
        DataSourcePlatform::Linux,
        &AtomicBool::new(false),
        "linux-policy-v1",
    )
    .expect("materialize Linux timeline");

    assert_eq!(projection.inserted_count, 1);
    assert_eq!(
        conn.query_row(
            "SELECT source_object_id FROM timeline_events WHERE event_type = 'FILE_MODIFIED'",
            [],
            |row| row.get::<_, String>(0)
        )
        .expect("read Linux timeline event"),
        "writable-system-path"
    );
}

#[test]
fn windows_projection_excludes_system_files_and_system_roots() {
    let conn = projection_connection();
    conn.execute_batch(
        "INSERT INTO file_entries
         (id, data_source_id, path, name, entry_type, system, read_only, modified_at)
         VALUES
         ('user-file', 'ds-1', 'Users/Alice/readonly.txt', 'readonly.txt', 'file', 0, 1, '2026-07-18T00:00:00Z'),
         ('native-system', 'ds-1', 'Users/Alice/system.dat', 'system.dat', 'file', 1, 0, '2026-07-18T00:00:00Z'),
         ('windows-root', 'ds-1', '[P3]/Windows/System32/kernel.dll', 'kernel.dll', 'file', 0, 0, '2026-07-18T00:00:00Z'),
         ('program-data', 'ds-1', '[P3]\\ProgramData\\service.db', 'service.db', 'file', 0, 0, '2026-07-18T00:00:00Z'),
         ('ntfs-metadata', 'ds-1', '$MFT', '$MFT', 'file', 0, 0, '2026-07-18T00:00:00Z');",
    )
    .expect("insert Windows file policy fixtures");

    let projection = materialize_file_activity_with_identity(
        &conn,
        DataSourcePlatform::Windows,
        &AtomicBool::new(false),
        "windows-policy-v1",
    )
    .expect("materialize Windows timeline");

    assert_eq!(projection.inserted_count, 1);
    assert_eq!(
        conn.query_row(
            "SELECT source_object_id FROM timeline_events WHERE event_type = 'FILE_MODIFIED'",
            [],
            |row| row.get::<_, String>(0)
        )
        .expect("read Windows timeline event"),
        "user-file"
    );
}

#[test]
fn projection_materializes_file_lifecycle_without_treating_access_as_execution() {
    let conn = projection_connection();
    conn.execute_batch(
        "INSERT INTO file_entries
         (id, data_source_id, path, name, entry_type, deleted,
          created_at, modified_at, accessed_at, changed_at)
         VALUES
         ('deleted-file', 'ds-1', '/home/user/tool.bin', 'tool.bin', 'file', 1,
          '2026-07-18T00:00:00Z', '2026-07-18T01:00:00Z',
          '2026-07-18T02:00:00Z', '2026-07-18T03:00:00Z');",
    )
    .expect("insert file lifecycle fixture");

    let projection = materialize_file_activity_unknown(&conn).expect("materialize file activity");
    let mut statement = conn
        .prepare(
            "SELECT event_type, parser_id, confidence, attrs
             FROM timeline_events
             ORDER BY ts ASC",
        )
        .expect("prepare file activity query");
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f32>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .expect("query file activity")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect file activity");

    assert_eq!(projection.inserted_count, 4);
    assert_eq!(
        rows.iter().map(|row| row.0.as_str()).collect::<Vec<_>>(),
        vec![
            "FILE_CREATED",
            "FILE_MODIFIED",
            "FILE_ACCESSED",
            "FILE_DELETED"
        ]
    );
    assert!(rows[2].3.contains("does not prove execution"));
    assert_eq!(rows[3].1, "timeline.file_deleted");
    assert_eq!(rows[3].2, 0.65);
}

#[test]
fn projection_without_graph_schema_reports_graph_as_not_applicable() {
    let conn = projection_connection();
    conn.execute_batch(
        "DROP TABLE graph_edges;
         DROP TABLE graph_nodes;
         INSERT INTO file_entries
         (id, data_source_id, path, name, entry_type, modified_at)
         VALUES ('file-1', 'ds-1', '/file-1', 'file-1', 'file', '2026-07-18T00:00:00Z');",
    )
    .expect("create projection-only schema");

    let first = materialize_file_activity_unknown(&conn).expect("materialize file timeline");
    let second = materialize_file_activity_unknown(&conn).expect("reuse file timeline");

    assert!(first.graph_complete);
    assert!(second.graph_complete);
    assert!(second.already_projected);
}

#[test]
fn missing_source_graph_node_keeps_projection_retryable() {
    let conn = persistence_sqlite::connection::open_in_memory().expect("open application database");
    persistence_sqlite::runner::run_all(&conn).expect("run application migrations");
    conn.execute_batch(
        "INSERT INTO cases
         (id, name, created_at, updated_at)
         VALUES ('case-1', 'Timeline case', '2026-07-18T00:00:00Z', '2026-07-18T00:00:00Z');
         INSERT INTO data_sources
         (id, case_id, name, kind, source_path)
         VALUES ('ds-1', 'case-1', 'Source', 'raw', 'C:/evidence/source.raw');",
    )
    .expect("insert application case and source");
    conn.execute(
        "INSERT INTO file_entries
         (id, data_source_id, path, name, entry_type, modified_at)
         VALUES ('file-1', 'ds-1', '/file-1', 'file-1', 'file', '2026-07-18T00:00:00Z')",
        [],
    )
    .expect("insert file without graph projection");

    let first = materialize_file_activity_unknown(&conn).expect("project file timeline");
    let second = materialize_file_activity_unknown(&conn).expect("observe completed projection");

    assert_eq!(first.inserted_count, 1);
    assert!(!first.graph_complete);
    assert_eq!(first.warnings.len(), 1);
    assert!(first.warnings[0].contains("not materialized"));
    assert!(!second.already_projected);
    assert!(!second.graph_complete);
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM graph_nodes", [], |row| {
            row.get::<_, u64>(0)
        })
        .expect("count timeline graph nodes"),
        1
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM graph_edges", [], |row| {
            row.get::<_, u64>(0)
        })
        .expect("count timeline graph edges"),
        0
    );

    conn.execute(
        "INSERT INTO graph_nodes
         (id, case_id, node_type, label, summary, tags, created_at)
         VALUES (
             'file-1', 'case-1', 'file', 'file-1', '', '[]',
             '2026-07-18T00:00:00Z'
         )",
        [],
    )
    .expect("materialize source graph node");
    let completed = materialize_file_activity_unknown(&conn).expect("retry graph projection");
    assert!(completed.graph_complete);
    assert!(completed.warnings.is_empty());
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM graph_edges", [], |row| {
            row.get::<_, u64>(0)
        })
        .expect("count completed timeline graph edges"),
        1
    );
}

#[test]
fn changed_projection_identity_replaces_stale_file_events() {
    let conn = projection_connection();
    conn.execute(
        "INSERT INTO file_entries
         (id, data_source_id, path, name, entry_type, modified_at)
         VALUES ('file-1', 'ds-1', '/file-1', 'file-1', 'file', '2026-07-18T00:00:00Z')",
        [],
    )
    .expect("insert file");

    materialize_file_activity_unknown_with_cancel_and_identity(
        &conn,
        &AtomicBool::new(false),
        "catalog-artifact-v1",
    )
    .expect("project first identity");
    conn.execute(
        "UPDATE file_entries
         SET modified_at = '2026-07-18T01:00:00Z'
         WHERE id = 'file-1'",
        [],
    )
    .expect("change source timestamp");
    let second = materialize_file_activity_unknown_with_cancel_and_identity(
        &conn,
        &AtomicBool::new(false),
        "catalog-artifact-v2",
    )
    .expect("replace stale projection");

    assert!(!second.already_projected);
    assert_eq!(
        conn.query_row(
            "SELECT ts FROM timeline_events WHERE parser_id = 'timeline.file_modified'",
            [],
            |row| row.get::<_, String>(0)
        )
        .expect("read replaced timestamp"),
        "2026-07-18T01:00:00Z"
    );
    assert_eq!(
        conn.query_row(
            "SELECT input_identity
             FROM timeline_projection_meta
             WHERE projection_key = 'file_activity_v2'",
            [],
            |row| row.get::<_, String>(0)
        )
        .expect("read projection identity"),
        "catalog-artifact-v2"
    );
}

#[test]
fn graph_write_failure_is_non_fatal_to_file_projection() {
    let conn = projection_connection();
    conn.execute_batch(
        "INSERT INTO file_entries
         (id, data_source_id, path, name, entry_type, modified_at)
         VALUES ('file-1', 'ds-1', '/file-1', 'file-1', 'file', '2026-07-18T00:00:00Z');
         CREATE TRIGGER reject_timeline_graph_edge
         BEFORE INSERT ON graph_edges
         BEGIN
             SELECT RAISE(FAIL, 'forced timeline graph failure');
         END;",
    )
    .expect("create graph failure fixture");

    let projection = materialize_file_activity_unknown(&conn).expect("project file timeline");

    assert_eq!(projection.inserted_count, 1);
    assert!(!projection.graph_complete);
    assert_eq!(projection.warnings.len(), 1);
    assert!(projection.warnings[0].contains("Timeline graph population failed"));
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM timeline_events", [], |row| {
            row.get::<_, u64>(0)
        })
        .expect("count timeline rows"),
        1
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM graph_nodes", [], |row| {
            row.get::<_, u64>(0)
        })
        .expect("count graph nodes"),
        0
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM graph_edges", [], |row| {
            row.get::<_, u64>(0)
        })
        .expect("count graph edges"),
        0
    );
    assert_eq!(
        conn.query_row(
            "SELECT status FROM timeline_projection_meta WHERE projection_key = 'file_activity_v2'",
            [],
            |row| row.get::<_, String>(0)
        )
        .expect("read projection status"),
        "done"
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM timeline_projection_meta
             WHERE projection_key = 'timeline_graph_v3'",
            [],
            |row| row.get::<_, u64>(0)
        )
        .expect("count graph projection state"),
        0
    );

    conn.execute_batch("DROP TRIGGER reject_timeline_graph_edge")
        .expect("remove graph failure fixture");
    let retry = materialize_file_activity_unknown(&conn).expect("retry graph projection");

    assert!(!retry.already_projected);
    assert!(retry.graph_complete);
    assert!(retry.warnings.is_empty());
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM graph_nodes", [], |row| {
            row.get::<_, u64>(0)
        })
        .expect("count graph nodes after retry"),
        1
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM graph_edges", [], |row| {
            row.get::<_, u64>(0)
        })
        .expect("count graph edges after retry"),
        1
    );
    assert_eq!(
        conn.query_row(
            "SELECT status FROM timeline_projection_meta WHERE projection_key = 'timeline_graph_v3'",
            [],
            |row| row.get::<_, String>(0)
        )
        .expect("read graph projection status"),
        "done"
    );
}

#[test]
fn timeline_graph_commits_completed_batches_and_retries_idempotently() {
    let conn = projection_connection();
    for id in ["event-a", "event-b", "event-c", "event-d"] {
        insert_timeline_row(&conn, id, "file");
    }
    conn.execute_batch(
        "CREATE TRIGGER reject_late_timeline_graph_edge
         BEFORE INSERT ON graph_edges
         WHEN NEW.source_id = 'event-c'
         BEGIN
             SELECT RAISE(FAIL, 'forced late timeline graph failure');
         END;",
    )
    .expect("create late graph failure fixture");

    let error = populate_timeline_event_graph_with_batch(&conn, 2)
        .expect_err("second graph batch should fail");

    assert!(error
        .to_string()
        .contains("forced late timeline graph failure"));
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM graph_nodes", [], |row| {
            row.get::<_, u64>(0)
        })
        .expect("count committed graph nodes"),
        2
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM graph_edges", [], |row| {
            row.get::<_, u64>(0)
        })
        .expect("count committed graph edges"),
        2
    );

    conn.execute_batch("DROP TRIGGER reject_late_timeline_graph_edge")
        .expect("remove late graph failure fixture");
    populate_timeline_event_graph_with_batch(&conn, 2).expect("retry graph projection");
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM graph_nodes", [], |row| {
            row.get::<_, u64>(0)
        })
        .expect("count graph nodes after retry"),
        4
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM graph_edges", [], |row| {
            row.get::<_, u64>(0)
        })
        .expect("count graph edges after retry"),
        4
    );
}

#[test]
fn cancelled_timeline_projection_does_not_claim_completion() {
    let conn = projection_connection();
    conn.execute(
        "INSERT INTO file_entries
         (id, data_source_id, path, name, entry_type, created_at)
         VALUES ('file-1', 'ds-1', '/file-1', 'file-1', 'file', '2026-07-18T00:00:00Z')",
        [],
    )
    .expect("insert file");
    let cancel_token = AtomicBool::new(true);

    let error = materialize_file_activity_unknown_with_cancel(&conn, &cancel_token)
        .expect_err("cancelled projection should stop");

    assert!(matches!(error, TimelineServiceError::Cancelled));
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name = 'timeline_projection_meta'",
            [],
            |row| row.get::<_, u64>(0)
        )
        .expect("count projection metadata table"),
        0
    );
}
