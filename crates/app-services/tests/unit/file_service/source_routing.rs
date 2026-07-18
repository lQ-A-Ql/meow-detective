use super::*;
use crate::file_service::PreparedSourceReadState;
use persistence_sqlite::repositories::{
    case_repo::CaseRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
};
use sha2::{Digest, Sha256};
use std::sync::Arc;

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

#[test]
#[ignore = "requires a retained PVE case with a ready derived RBD source"]
fn retained_pve_rbd_source_context_reads_analysis_candidate() {
    let case_root = std::env::var_os("FORENSICS_PVE_RBD_PREVIEW_CASE_ROOT")
        .map(std::path::PathBuf::from)
        .expect("FORENSICS_PVE_RBD_PREVIEW_CASE_ROOT must point to a retained PVE case");
    let case_conn = persistence_sqlite::connection::open_existing(&case_root.join("app.db"))
        .expect("open retained PVE case database");
    let cases = CaseRepo::new(&case_conn)
        .list_all()
        .expect("query retained PVE case");
    assert_eq!(cases.len(), 1, "retained PVE case count");
    let case_id = cases[0].id.clone();
    let source = DataSourceRepo::new(&case_conn)
        .find_by_case(&case_id)
        .expect("query retained data sources")
        .into_iter()
        .find(|source| source.kind == domain::DataSourceKind::CephRbd)
        .expect("ready derived RBD source");
    let source_conn =
        crate::source_db::open_registered_source_db(&case_conn, &case_root, &source.id)
            .expect("open derived RBD source database");
    let file_id = source_conn
        .query_row(
            "SELECT id
             FROM file_entries
             WHERE data_source_id = ?1
               AND entry_type = 'file'
               AND (lower(path) = 'etc/passwd' OR lower(path) LIKE '%/etc/passwd')
             ORDER BY path
             LIMIT 1",
            [&source.id.0],
            |row| row.get::<_, String>(0),
        )
        .map(FileEntryId)
        .expect("find derived VM /etc/passwd");

    let mut context =
        SourceReadContext::new(&source_conn, &case_conn, &case_root, &case_id, &source.id);
    let bytes = context
        .read_file_header_by_id(&file_id, 4 * 1024)
        .expect("read derived VM analysis candidate");
    let second_file_id = source_conn
        .query_row(
            "SELECT id
             FROM file_entries
             WHERE data_source_id = ?1
               AND entry_type = 'file'
               AND (lower(path) = 'etc/group' OR lower(path) LIKE '%/etc/group')
             ORDER BY path
             LIMIT 1",
            [&source.id.0],
            |row| row.get::<_, String>(0),
        )
        .map(FileEntryId)
        .expect("find derived VM /etc/group");
    let second_bytes = context
        .read_file_header_by_id(&second_file_id, 4 * 1024)
        .expect("read second derived VM analysis candidate");

    assert_eq!(bytes.len(), 1_019);
    assert!(!second_bytes.is_empty());
    assert_eq!(
        context.filesystem_read_metrics().filesystem_open_operations,
        1,
        "one source-bound analysis context must reuse its prepared filesystem"
    );
    let expected_sha256 = "be6b8d46d8cdb839b738efda17b7b5841a990786cc6ecc869386c266de25582d";
    assert_eq!(hex::encode(Sha256::digest(&bytes)), expected_sha256);

    let runtime = crate::ceph_reconstruction::build_derived_rbd_runtime(
        &case_conn, &case_root, &case_id, &source.id,
    )
    .map(Arc::new)
    .expect("build shared derived runtime");
    let mut prepared = PreparedSourceReadState::new(case_id.0.clone(), source.id.clone(), runtime);
    let prepared_bytes = prepared
        .read_file_header_by_id(&source_conn, &file_id, 4 * 1024)
        .expect("read candidate through prepared runtime");
    let prepared_second_bytes = prepared
        .read_file_header_by_id(&source_conn, &second_file_id, 4 * 1024)
        .expect("read second candidate through prepared runtime");

    assert_eq!(prepared_bytes, bytes);
    assert_eq!(prepared_second_bytes, second_bytes);
    assert_eq!(
        hex::encode(Sha256::digest(&prepared_bytes)),
        expected_sha256
    );
}
