use super::*;
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};

fn setup_case_conn() -> rusqlite::Connection {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE data_sources (
            id TEXT PRIMARY KEY NOT NULL,
            case_id TEXT NOT NULL,
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            source_path TEXT NOT NULL,
            imported_at TEXT NOT NULL DEFAULT (datetime('now')),
            source_hash_sha256 TEXT,
            hash_status TEXT DEFAULT 'unknown',
            canonical_source_path TEXT,
            evidence_size INTEGER,
            reader_kind TEXT,
            provenance_status TEXT DEFAULT 'unknown',
            provenance_warnings TEXT DEFAULT '[]',
            storage_model TEXT NOT NULL DEFAULT 'source_db',
            source_db_rel_path TEXT,
            index_rel_path TEXT,
            staging_rel_path TEXT,
            platform TEXT NOT NULL DEFAULT 'unknown',
            profile TEXT,
            import_state TEXT NOT NULL DEFAULT 'pending',
            schema_version TEXT,
            last_error TEXT
        );",
    )
    .unwrap();
    conn
}

fn insert_data_source(conn: &rusqlite::Connection, id: &str, name: &str) {
    let ds = domain::DataSource {
        id: DataSourceId(id.to_string()),
        name: name.to_string(),
        kind: domain::DataSourceKind::LogicalDirectory,
        source_path: std::path::PathBuf::from(format!("D:/{name}")),
        imported_at: chrono::Utc::now(),
        provenance: domain::DataSourceProvenance::unknown(),
    };
    DataSourceRepo::new(conn)
        .insert_with_storage(
            &domain::CaseId("case-1".to_string()),
            &ds,
            &DataSourceStorage::source_db(id, Some("linux"), None),
        )
        .unwrap();
    DataSourceRepo::new(conn)
        .update_import_state(&ds.id, "ready", None)
        .unwrap();
}

fn seed_source_db(case_root: &Path, data_source_id: &str) {
    let conn =
        crate::source_db::open_source_db(case_root, &DataSourceId(data_source_id.to_string()))
            .unwrap();
    conn.execute(
        "INSERT INTO file_entries
         (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system)
         VALUES ('file-1', NULL, ?1, '/', '/', 'directory', NULL, NULL, 0, 0, 0)",
        [data_source_id],
    )
    .unwrap();
}

#[test]
fn tree_wraps_duplicate_local_ids_by_data_source() {
    let tmp = tempfile::TempDir::new().unwrap();
    let case_conn = setup_case_conn();
    insert_data_source(&case_conn, "ds-a", "Source A");
    insert_data_source(&case_conn, "ds-b", "Source B");
    seed_source_db(tmp.path(), "ds-a");
    seed_source_db(tmp.path(), "ds-b");
    let roots = get_file_tree_for_case(
        &case_conn,
        tmp.path(),
        &domain::CaseId("case-1".to_string()),
        false,
    )
    .unwrap();
    let ids = roots.into_iter().map(|node| node.id).collect::<Vec<_>>();
    assert!(ids.contains(&"ds:ds-a:file-1".to_string()));
    assert!(ids.contains(&"ds:ds-b:file-1".to_string()));
}
