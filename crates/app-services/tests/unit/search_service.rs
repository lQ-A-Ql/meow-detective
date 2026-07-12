use super::*;
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, file_repo::FileRepo};
use search::ExtractedText;
use std::io::Cursor;
use tempfile::TempDir;
fn setup_file_db() -> (rusqlite::Connection, Vec<FileEntryId>) {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!(
        "../../../persistence-sqlite/src/migrations/scripts/0001_cases.sql"
    ))
    .unwrap();
    conn.execute_batch(include_str!(
        "../../../persistence-sqlite/src/migrations/scripts/0002_data_sources.sql"
    ))
    .unwrap();
    conn.execute_batch(include_str!(
        "../../../persistence-sqlite/src/migrations/scripts/0003_file_entries.sql"
    ))
    .unwrap();
    conn.execute_batch(include_str!(
        "../../../persistence-sqlite/src/migrations/scripts/0022_file_entry_visibility_flags.sql"
    ))
    .unwrap();
    conn.execute(
        "INSERT INTO cases (id, name, created_at, updated_at)
         VALUES ('case-1', 'Case', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at)
         VALUES ('ds-1', 'case-1', 'sample', 'LogicalDirectory', 'C:/sample', '2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    let ids = vec![
        FileEntryId("file-1".to_string()),
        FileEntryId("file-2".to_string()),
    ];
    let entries = ids
        .iter()
        .map(|id| FileEntry {
            id: id.clone(),
            parent_id: None,
            data_source_id: DataSourceId("ds-1".to_string()),
            path: format!("/{}.txt", id.0),
            name: format!("{}.txt", id.0),
            entry_type: EntryType::File,
            size: Some(32),
            ext: Some("txt".to_string()),
            deleted: false,
            hidden: false,
            system: false,
            encrypted: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        })
        .collect::<Vec<_>>();
    FileRepo::new(&conn).insert_batch(&entries).unwrap();
    (conn, ids)
}

fn setup_case_db_with_source(tmp: &TempDir) -> rusqlite::Connection {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    let case = domain::CaseMeta {
        id: domain::CaseId("case-1".to_string()),
        name: "case".to_string(),
        number: None,
        examiner: None,
        notes: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    persistence_sqlite::repositories::case_repo::CaseRepo::new(&conn)
        .create(&case)
        .unwrap();
    let ds = domain::DataSource {
        id: DataSourceId("ds-1".to_string()),
        name: "source".to_string(),
        kind: domain::DataSourceKind::LogicalDirectory,
        source_path: tmp.path().join("source"),
        imported_at: chrono::Utc::now(),
        provenance: domain::DataSourceProvenance::unknown(),
    };
    DataSourceRepo::new(&conn)
        .insert(&domain::CaseId("case-1".to_string()), &ds)
        .unwrap();
    conn.execute_batch("UPDATE data_sources SET import_state='ready',platform='linux'")
        .unwrap();
    conn
}

fn metric_value(report: &PerformanceReportDto, key: &str) -> Option<f64> {
    report
        .metrics
        .iter()
        .find(|metric| metric.key == key)
        .map(|metric| metric.value)
}

#[test]
fn search_index_instrumentation_reports_bounded_metrics() {
    let (conn, ids) = setup_file_db();
    let tmp = TempDir::new().unwrap();
    let result = index_files_instrumented(&conn, tmp.path(), &ids, |id| {
        Some(Box::new(Cursor::new(format!("alpha beta {}", id.0))))
    })
    .unwrap();
    assert_eq!(result.stats.indexed_count, 2);
    assert_eq!(
        metric_value(&result.performance_report, "search.index.rows"),
        Some(2.0)
    );
    assert!(metric_value(&result.performance_report, "search.index.elapsedMs").is_some());
    assert!(result
        .performance_report
        .metrics
        .iter()
        .all(|metric| !metric.key.contains("path")));
}

#[test]
fn search_query_instrumentation_reports_query_metrics() {
    let (conn, ids) = setup_file_db();
    let tmp = TempDir::new().unwrap();
    index_files_instrumented(&conn, tmp.path(), &ids, |id| {
        Some(Box::new(Cursor::new(format!("needle haystack {}", id.0))))
    })
    .unwrap();

    let result = search_files_real_instrumented(tmp.path(), "needle", 0, 10).unwrap();

    assert_eq!(result.page.items.len(), 2);
    assert_eq!(
        metric_value(&result.performance_report, "search.query.rows"),
        Some(2.0)
    );
    assert_eq!(
        metric_value(&result.performance_report, "search.query.totalRows"),
        Some(2.0)
    );
    assert!(result
        .performance_report
        .summary
        .summary
        .starts_with("Search query returned 2 rows"));
}

#[test]
fn search_files_for_case_reads_source_indexes_and_wraps_file_ids() {
    let tmp = TempDir::new().unwrap();
    let case_conn = setup_case_db_with_source(&tmp);
    let index_dir =
        crate::source_db::source_index_dir(tmp.path(), &DataSourceId("ds-1".to_string()));
    let index = SearchIndex::create(&index_dir).unwrap();
    index
        .index_documents(
            &[ExtractedText {
                file_id: "file-1".to_string(),
                content: "needle source scoped content".to_string(),
                encoding: "utf-8".to_string(),
                extractable: true,
                byte_count: 28,
            }],
            &[("file-1".to_string(), "/evidence/file-1.txt".to_string())],
        )
        .unwrap();

    let page = search_files_for_case(
        &case_conn,
        tmp.path(),
        &domain::CaseId("case-1".to_string()),
        "needle",
        0,
        10,
    )
    .unwrap();

    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].file_id, "ds:ds-1:file-1");
    assert_eq!(page.items[0].path, "/evidence/file-1.txt");
}
