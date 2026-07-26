use super::*;
use domain::{
    DataSource, DataSourceId, DataSourceKind, DataSourceProvenance, EntryType, FileEntry,
    FileEntryId,
};
use persistence_sqlite::repositories::{
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    file_repo::FileRepo,
};
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
    conn.execute_batch(include_str!(
        "../../../persistence-sqlite/src/migrations/scripts/0042_file_entry_encrypted.sql"
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

fn register_ready_search_source(
    case_conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    source_id: &str,
) -> std::path::PathBuf {
    let source = DataSource {
        id: DataSourceId(source_id.to_string()),
        name: source_id.to_string(),
        kind: DataSourceKind::LogicalDirectory,
        source_path: case_root.join(format!("{source_id}.fixture")),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(source_id, Some("linux"), None);
    storage.import_state = "ready".to_string();
    let index_rel_path = storage.index_rel_path.clone().unwrap();
    DataSourceRepo::new(case_conn)
        .insert_with_storage(&domain::CaseId("case-1".to_string()), &source, &storage)
        .unwrap();
    case_root.join(index_rel_path)
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
    case_conn
        .execute(
            "UPDATE data_sources
             SET index_rel_path = 'registered-indexes/ds-1'
             WHERE id = 'ds-1'",
            [],
        )
        .unwrap();
    let index_dir = tmp.path().join("registered-indexes/ds-1");
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
    assert_eq!(page.available, 1);
    assert!(!page.truncated);
    assert_eq!(page.items[0].file_id, "ds:ds-1:file-1");
    assert_eq!(page.items[0].path, "/evidence/file-1.txt");
}

#[test]
fn case_search_deep_page_refills_stable_source_indexes_in_bounded_batches() {
    let tmp = TempDir::new().unwrap();
    let case_conn = setup_case_db_with_source(&tmp);
    let first_index_dir = tmp.path().join("sources/ds-1/index");
    let second_index_dir = register_ready_search_source(&case_conn, tmp.path(), "ds-2");
    let texts = (0..300)
        .map(|index| ExtractedText {
            file_id: format!("file-{index:03}"),
            content: "shared deep pagination token".to_string(),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 28,
        })
        .collect::<Vec<_>>();
    let paths = texts
        .iter()
        .map(|text| {
            (
                text.file_id.clone(),
                format!("/evidence/{}.txt", text.file_id),
            )
        })
        .collect::<Vec<_>>();
    SearchIndex::create(&first_index_dir)
        .unwrap()
        .index_documents(&texts, &paths)
        .unwrap();
    SearchIndex::create(&second_index_dir)
        .unwrap()
        .index_documents(&texts, &paths)
        .unwrap();

    let page = search_files_for_case(
        &case_conn,
        tmp.path(),
        &domain::CaseId("case-1".to_string()),
        "pagination",
        520,
        40,
    )
    .unwrap();

    assert_eq!(page.total, 600);
    assert_eq!(page.available, 600);
    assert!(!page.truncated);
    assert_eq!(page.items.len(), 40);
    assert_eq!(page.items.first().unwrap().file_id, "ds:ds-2:file-220");
    assert_eq!(page.items.last().unwrap().file_id, "ds:ds-2:file-259");
}

#[test]
fn case_search_window_caps_untrusted_offsets_without_overflow() {
    assert_eq!(case_search::bounded_scan_end(0, 50), 50);
    assert_eq!(
        case_search::bounded_scan_end(case_search::MAX_CASE_SEARCH_WINDOW - 10, u32::MAX,),
        case_search::MAX_CASE_SEARCH_WINDOW as usize
    );
    assert_eq!(
        case_search::bounded_scan_end(case_search::MAX_CASE_SEARCH_WINDOW, 50),
        0
    );
    assert_eq!(case_search::bounded_scan_end(u64::MAX, u32::MAX), 0);
}

#[test]
fn case_search_cursor_reads_each_ranked_hit_once_across_sources() {
    let tmp = TempDir::new().unwrap();
    let case_conn = setup_case_db_with_source(&tmp);
    let first_index_dir = tmp.path().join("sources/ds-1/index");
    let second_index_dir = register_ready_search_source(&case_conn, tmp.path(), "ds-2");
    let texts = (0..73)
        .rev()
        .map(|index| ExtractedText {
            file_id: format!("file-{index:03}"),
            content: "shared cursor pagination token".to_string(),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 30,
        })
        .collect::<Vec<_>>();
    let paths = texts
        .iter()
        .map(|text| {
            (
                text.file_id.clone(),
                format!("/evidence/{}.txt", text.file_id),
            )
        })
        .collect::<Vec<_>>();
    for index_dir in [&first_index_dir, &second_index_dir] {
        SearchIndex::create(index_dir)
            .unwrap()
            .index_documents(&texts, &paths)
            .unwrap();
    }

    let mut cursor = None;
    let mut file_ids = Vec::new();
    loop {
        let page = search_files_for_case_cursor(
            &case_conn,
            tmp.path(),
            &domain::CaseId("case-1".to_string()),
            "pagination",
            cursor.as_deref(),
            17,
        )
        .unwrap();
        assert_eq!(page.total, 146);
        file_ids.extend(page.items.into_iter().map(|item| item.file_id));
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }

    assert_eq!(file_ids.len(), 146);
    assert_eq!(
        file_ids
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        file_ids.len()
    );
    assert_eq!(
        file_ids.first().map(String::as_str),
        Some("ds:ds-1:file-000")
    );
    assert_eq!(
        file_ids.last().map(String::as_str),
        Some("ds:ds-2:file-072")
    );
}

#[test]
fn case_search_cursor_rejects_an_index_commit_between_pages() {
    let tmp = TempDir::new().unwrap();
    let case_conn = setup_case_db_with_source(&tmp);
    let index_dir = tmp.path().join("sources/ds-1/index");
    let index = SearchIndex::create(&index_dir).unwrap();
    let texts = ["file-1", "file-2"]
        .into_iter()
        .map(|file_id| ExtractedText {
            file_id: file_id.to_string(),
            content: "stable cursor token".to_string(),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 19,
        })
        .collect::<Vec<_>>();
    let paths = texts
        .iter()
        .map(|text| (text.file_id.clone(), format!("/{}.txt", text.file_id)))
        .collect::<Vec<_>>();
    index.index_documents(&texts, &paths).unwrap();
    let first = search_files_for_case_cursor(
        &case_conn,
        tmp.path(),
        &domain::CaseId("case-1".to_string()),
        "cursor",
        None,
        1,
    )
    .unwrap();
    let cursor = first.next_cursor.unwrap();
    index
        .index_documents(
            &[ExtractedText {
                file_id: "file-3".to_string(),
                content: "stable cursor token".to_string(),
                encoding: "utf-8".to_string(),
                extractable: true,
                byte_count: 19,
            }],
            &[("file-3".to_string(), "/file-3.txt".to_string())],
        )
        .unwrap();

    let error = search_files_for_case_cursor(
        &case_conn,
        tmp.path(),
        &domain::CaseId("case-1".to_string()),
        "cursor",
        Some(&cursor),
        1,
    )
    .unwrap_err();

    assert!(matches!(error, SearchError::InvalidInput(_)));
}

#[test]
fn case_search_cursor_rejects_an_equivalent_rebuilt_index_generation() {
    let tmp = TempDir::new().unwrap();
    let case_conn = setup_case_db_with_source(&tmp);
    let index_dir = tmp.path().join("sources/ds-1/index");
    let texts = ["file-1", "file-2"]
        .into_iter()
        .map(|file_id| ExtractedText {
            file_id: file_id.to_string(),
            content: "equivalent rebuilt cursor token".to_string(),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 31,
        })
        .collect::<Vec<_>>();
    let paths = texts
        .iter()
        .map(|text| (text.file_id.clone(), format!("/{}.txt", text.file_id)))
        .collect::<Vec<_>>();
    let first_index = SearchIndex::create(&index_dir).unwrap();
    first_index.index_documents(&texts, &paths).unwrap();
    let first_generation = first_index.generation().to_string();
    let first_opstamp = first_index.snapshot_opstamp().unwrap();
    drop(first_index);

    let first_page = search_files_for_case_cursor(
        &case_conn,
        tmp.path(),
        &domain::CaseId("case-1".to_string()),
        "rebuilt",
        None,
        1,
    )
    .unwrap();
    let cursor = first_page.next_cursor.unwrap();

    std::fs::remove_dir_all(&index_dir).unwrap();
    let rebuilt = SearchIndex::create(&index_dir).unwrap();
    rebuilt.index_documents(&texts, &paths).unwrap();
    assert_ne!(rebuilt.generation(), first_generation);
    assert_eq!(rebuilt.snapshot_opstamp().unwrap(), first_opstamp);
    drop(rebuilt);

    let error = search_files_for_case_cursor(
        &case_conn,
        tmp.path(),
        &domain::CaseId("case-1".to_string()),
        "rebuilt",
        Some(&cursor),
        1,
    )
    .unwrap_err();

    assert!(matches!(error, SearchError::InvalidInput(_)));
    assert!(error.to_string().contains("generation changed"));
}

#[test]
fn case_search_cursor_rejects_tampered_and_oversized_tokens() {
    let tmp = TempDir::new().unwrap();
    let case_conn = setup_case_db_with_source(&tmp);
    let index_dir = tmp.path().join("sources/ds-1/index");
    let index = SearchIndex::create(&index_dir).unwrap();
    let texts = ["file-1", "file-2"]
        .into_iter()
        .map(|file_id| ExtractedText {
            file_id: file_id.to_string(),
            content: "tamper cursor token".to_string(),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 19,
        })
        .collect::<Vec<_>>();
    let paths = texts
        .iter()
        .map(|text| (text.file_id.clone(), format!("/{}.txt", text.file_id)))
        .collect::<Vec<_>>();
    index.index_documents(&texts, &paths).unwrap();
    let first = search_files_for_case_cursor(
        &case_conn,
        tmp.path(),
        &domain::CaseId("case-1".to_string()),
        "tamper",
        None,
        1,
    )
    .unwrap();
    let mut tampered = first.next_cursor.unwrap().into_bytes();
    let payload_index = tampered.iter().position(|byte| *byte == b'.').unwrap() + 1;
    tampered[payload_index] = if tampered[payload_index] == b'A' {
        b'B'
    } else {
        b'A'
    };
    let tampered = String::from_utf8(tampered).unwrap();

    for cursor in [
        tampered,
        "x".repeat(transport::paging::MAX_OPAQUE_CURSOR_LENGTH + 1),
    ] {
        let error = search_files_for_case_cursor(
            &case_conn,
            tmp.path(),
            &domain::CaseId("case-1".to_string()),
            "tamper",
            Some(&cursor),
            1,
        )
        .unwrap_err();
        assert!(matches!(error, SearchError::InvalidInput(_)));
    }
}
