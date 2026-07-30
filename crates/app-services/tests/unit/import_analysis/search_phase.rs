use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use domain::DataSourcePlatform;
use rusqlite::params;
use tempfile::TempDir;

use super::setup_source_db;
use crate::import_analysis::{run_search_index_phase, SearchIndexPhaseOptions};

#[test]
fn dedicated_search_phase_never_creates_a_missing_source_database() {
    let tmp = TempDir::new().expect("create missing source fixture");
    let db_path = tmp.path().join("missing-source.db");
    let index_dir = tmp.path().join("search-index");
    let error = run_search_index_phase(SearchIndexPhaseOptions {
        db_path: db_path.clone(),
        data_source_id: domain::DataSourceId("missing-source".to_string()),
        platform: DataSourcePlatform::Linux,
        index_dir: index_dir.clone(),
        cancel_token: Arc::new(AtomicBool::new(false)),
    })
    .expect_err("missing source database must fail");

    assert!(error.to_string().contains("not found"));
    assert!(!db_path.exists());
    assert!(!index_dir.exists());
    assert!(!index_dir.with_extension("next").exists());
}

#[test]
fn dedicated_search_phase_indexes_every_metadata_row_without_evidence_reads() {
    let tmp = TempDir::new().expect("create search phase fixture");
    let (db_path, data_source_id) = setup_source_db(&tmp);
    let connection =
        persistence_sqlite::open_existing_source(&db_path).expect("open source database");
    connection
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, encrypted)
             VALUES
             ('directory', ?1, 'Downloads/report-data', 'report-data', 'directory', NULL, NULL, 0, 0, 0, 0),
             ('empty', ?1, 'Downloads/report.txt', 'report.txt', 'file', 0, 'txt', 0, 0, 0, 0),
             ('huge', ?1, 'Downloads/archive.7z', 'archive.7z', 'file', ?2, '7z', 0, 0, 0, 1)",
            params![data_source_id.0, 3_u64 * 1024 * 1024 * 1024],
        )
        .expect("insert metadata rows");
    drop(connection);

    let index_dir = tmp.path().join("search-index");
    let stats = run_search_index_phase(SearchIndexPhaseOptions {
        db_path,
        data_source_id,
        platform: DataSourcePlatform::Linux,
        index_dir: index_dir.clone(),
        cancel_token: Arc::new(AtomicBool::new(false)),
    })
    .expect("run metadata search phase");

    assert_eq!(stats.eligible_count, 3);
    assert_eq!(stats.indexed_count, 3);
    assert_eq!(stats.failed_count, 0);
    assert_eq!(stats.skipped_count, 0);
    let index = search::SearchIndex::open(&index_dir).expect("open metadata index");
    assert_eq!(index.document_count().unwrap(), 3);
    let options = search::FileSearchOptions {
        query: "report".to_string(),
        ..Default::default()
    };
    let session = index.file_query_session(&options).unwrap();
    assert_eq!(session.rank_after(None, 10).unwrap().total_count, 2);
}
