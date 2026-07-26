use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use domain::DataSourcePlatform;
use rusqlite::params;
use tempfile::TempDir;

use super::setup_source_db;
use crate::import_analysis::{
    run_search_index_phase, search_phase::search_candidate_page_sql, SearchIndexPhaseOptions,
};

#[test]
fn search_candidate_query_uses_priority_keyset_without_offset() {
    let sql = search_candidate_page_sql().to_ascii_uppercase();

    assert!(sql.contains("ORDER BY PRIORITY_RANK ASC, PATH ASC, ID ASC"));
    assert!(sql.contains("PRIORITY_RANK > ?4"));
    assert!(sql.contains("PATH > ?5"));
    assert!(sql.contains("ID > ?6"));
    assert!(!sql.contains(" OFFSET "));
}

#[test]
fn dedicated_search_phase_never_creates_a_missing_source_database() {
    let tmp = TempDir::new().expect("create missing source fixture");
    let db_path = tmp.path().join("missing-source.db");
    let index_dir = tmp.path().join("search-index");
    let error = run_search_index_phase(SearchIndexPhaseOptions {
        case_root: tmp.path().to_path_buf(),
        db_path: db_path.clone(),
        case_id: "case-1".to_string(),
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
fn dedicated_search_phase_indexes_linux_passwd_content_with_truthful_stats() {
    let tmp = TempDir::new().expect("create search phase fixture");
    let evidence = tmp.path().join("evidence");
    let etc = evidence.join("etc");
    std::fs::create_dir_all(&etc).expect("create etc fixture");
    let passwd =
        b"root:x:0:0:root:/root:/bin/bash\ndaemon:x:1:1:daemon:/usr/sbin:/usr/sbin/nologin\n";
    std::fs::write(etc.join("passwd"), passwd).expect("write passwd fixture");
    std::fs::write(etc.join("empty.txt"), []).expect("write empty fixture");

    let (db_path, data_source_id) = setup_source_db(&tmp);
    let connection =
        persistence_sqlite::open_existing_source(&db_path).expect("open source database");
    connection
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, encrypted)
             VALUES
             ('passwd', ?1, 'etc/passwd', 'passwd', 'file', ?2, NULL, 0, 0, 0, 0),
             ('empty', ?1, 'etc/empty.txt', 'empty.txt', 'file', 0, 'txt', 0, 0, 0, 0)",
            params![data_source_id.0, passwd.len() as u64],
        )
        .expect("insert search candidates");
    drop(connection);

    let index_dir = tmp.path().join("search-index");
    let options = SearchIndexPhaseOptions {
        case_root: tmp.path().to_path_buf(),
        db_path,
        case_id: "case-1".to_string(),
        data_source_id,
        platform: DataSourcePlatform::Linux,
        index_dir: index_dir.clone(),
        cancel_token: Arc::new(AtomicBool::new(false)),
    };
    let stats = run_search_index_phase(options.clone()).expect("run dedicated search phase");
    let retry_stats = run_search_index_phase(options).expect("retry dedicated search phase");

    assert_eq!(stats.eligible_count, 2);
    assert_eq!(stats.indexed_count, 1);
    assert_eq!(stats.skipped_count, 1);
    assert_eq!(stats.failed_count, 0);
    assert_eq!(retry_stats, stats);
    let result = search::SearchIndex::open(&index_dir)
        .expect("open search index")
        .search("nologin", 10)
        .expect("search passwd content");
    assert_eq!(result.total_count, 1);
    let passwd_hit = result
        .hits
        .iter()
        .find(|hit| hit.path.replace('\\', "/").ends_with("etc/passwd"))
        .expect("find passwd content hit");
    assert!(passwd_hit
        .snippets
        .iter()
        .any(|snippet| snippet.text.contains("nologin")));
}

#[test]
fn sql_eligibility_matches_extension_and_linux_basename_policy() {
    let tmp = TempDir::new().expect("create search eligibility fixture");
    let evidence = tmp.path().join("evidence");
    let etc = evidence.join("etc");
    std::fs::create_dir_all(&etc).expect("create evidence fixture");
    std::fs::write(etc.join("UPPER.TXT"), "upper extension marker")
        .expect("write extension fallback fixture");
    std::fs::write(etc.join("spaced-passwd"), "spaced basename marker")
        .expect("write linux basename fixture");

    let (db_path, data_source_id) = setup_source_db(&tmp);
    let connection =
        persistence_sqlite::open_existing_source(&db_path).expect("open source database");
    connection
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, encrypted)
             VALUES
             ('upper', ?1, 'etc/UPPER.TXT', 'UPPER.TXT', 'file', 22, NULL, 0, 0, 0, 0),
             ('spaced-passwd', ?1, 'etc/spaced-passwd', ' passwd ', 'file', 22, '', 0, 0, 0, 0),
             ('ext-override', ?1, 'etc/missing-override', 'looks.txt', 'file', 10, 'bin', 0, 0, 0, 0),
             ('too-large', ?1, 'etc/missing-large.txt', 'missing-large.txt', 'file', ?2, 'txt', 0, 0, 0, 0),
             ('unknown-size', ?1, 'etc/missing-size.txt', 'missing-size.txt', 'file', NULL, 'txt', 0, 0, 0, 0)",
            params![
                data_source_id.0,
                infrastructure::constants::IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES + 1
            ],
        )
        .expect("insert eligibility candidates");
    drop(connection);

    let stats = run_search_index_phase(SearchIndexPhaseOptions {
        case_root: tmp.path().to_path_buf(),
        db_path,
        case_id: "case-1".to_string(),
        data_source_id,
        platform: DataSourcePlatform::Linux,
        index_dir: tmp.path().join("search-index"),
        cancel_token: Arc::new(AtomicBool::new(false)),
    })
    .expect("run eligibility-aware search phase");

    assert_eq!(stats.eligible_count, 2);
    assert_eq!(stats.indexed_count, 2);
    assert_eq!(stats.skipped_count, 0);
    assert_eq!(stats.failed_count, 0);
}

#[test]
fn windows_sql_eligibility_excludes_extensionless_linux_basename() {
    let tmp = TempDir::new().expect("create Windows eligibility fixture");
    let (db_path, data_source_id) = setup_source_db(&tmp);
    let connection =
        persistence_sqlite::open_existing_source(&db_path).expect("open source database");
    connection
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, encrypted)
             VALUES ('passwd', ?1, 'etc/passwd', 'passwd', 'file', 12, NULL, 0, 0, 0, 0)",
            [&data_source_id.0],
        )
        .expect("insert Linux-only candidate");
    drop(connection);

    let stats = run_search_index_phase(SearchIndexPhaseOptions {
        case_root: tmp.path().to_path_buf(),
        db_path,
        case_id: "case-1".to_string(),
        data_source_id,
        platform: DataSourcePlatform::Windows,
        index_dir: tmp.path().join("search-index"),
        cancel_token: Arc::new(AtomicBool::new(false)),
    })
    .expect("run Windows search phase");

    assert_eq!(stats.eligible_count, 0);
    assert_eq!(stats.indexed_count, 0);
    assert_eq!(stats.skipped_count, 0);
    assert_eq!(stats.failed_count, 0);
}

#[test]
fn failed_search_generation_preserves_the_previous_complete_index() {
    let tmp = TempDir::new().expect("create search phase fixture");
    let evidence = tmp.path().join("evidence");
    let etc = evidence.join("etc");
    std::fs::create_dir_all(&etc).expect("create etc fixture");
    let passwd =
        b"root:x:0:0:root:/root:/bin/bash\ndaemon:x:1:1:daemon:/usr/sbin:/usr/sbin/nologin\n";
    std::fs::write(etc.join("passwd"), passwd).expect("write passwd fixture");

    let (db_path, data_source_id) = setup_source_db(&tmp);
    let connection =
        persistence_sqlite::open_existing_source(&db_path).expect("open source database");
    connection
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, encrypted)
             VALUES ('passwd', ?1, 'etc/passwd', 'passwd', 'file', ?2, NULL, 0, 0, 0, 0)",
            params![data_source_id.0, passwd.len() as u64],
        )
        .expect("insert initial search candidate");
    drop(connection);

    let index_dir = tmp.path().join("search-index");
    let options = SearchIndexPhaseOptions {
        case_root: tmp.path().to_path_buf(),
        db_path: db_path.clone(),
        case_id: "case-1".to_string(),
        data_source_id: data_source_id.clone(),
        platform: DataSourcePlatform::Linux,
        index_dir: index_dir.clone(),
        cancel_token: Arc::new(AtomicBool::new(false)),
    };
    run_search_index_phase(options.clone()).expect("build initial complete index");

    let connection =
        persistence_sqlite::open_existing_source(&db_path).expect("reopen source database");
    connection
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, encrypted)
             VALUES ('missing', ?1, 'etc/missing.txt', 'missing.txt', 'file', 12, 'txt', 0, 0, 0, 0)",
            [&data_source_id.0],
        )
        .expect("insert unreadable search candidate");
    drop(connection);

    let error = run_search_index_phase(options).expect_err("failed generation must not publish");
    assert!(error.to_string().contains("previous index was preserved"));
    let result = search::SearchIndex::open(&index_dir)
        .expect("open preserved search index")
        .search("nologin", 10)
        .expect("query preserved search index");
    assert_eq!(result.total_count, 1);
    assert!(result
        .hits
        .iter()
        .any(|hit| hit.path.replace('\\', "/").ends_with("etc/passwd")));
    assert!(!index_dir.with_extension("next").exists());
}

#[test]
fn interrupted_search_publish_restores_previous_generation_before_retry() {
    let tmp = TempDir::new().expect("create interrupted search fixture");
    let evidence = tmp.path().join("evidence");
    let etc = evidence.join("etc");
    std::fs::create_dir_all(&etc).expect("create etc fixture");
    let passwd = b"root:x:0:0:root:/root:/bin/bash\n";
    std::fs::write(etc.join("passwd"), passwd).expect("write passwd fixture");

    let (db_path, data_source_id) = setup_source_db(&tmp);
    let connection =
        persistence_sqlite::open_existing_source(&db_path).expect("open source database");
    connection
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, encrypted)
             VALUES ('passwd', ?1, 'etc/passwd', 'passwd', 'file', ?2, NULL, 0, 0, 0, 0)",
            params![data_source_id.0, passwd.len() as u64],
        )
        .expect("insert search candidate");
    drop(connection);

    let index_dir = tmp.path().join("search-index");
    let base_options = SearchIndexPhaseOptions {
        case_root: tmp.path().to_path_buf(),
        db_path,
        case_id: "case-1".to_string(),
        data_source_id,
        platform: DataSourcePlatform::Linux,
        index_dir: index_dir.clone(),
        cancel_token: Arc::new(AtomicBool::new(false)),
    };
    run_search_index_phase(base_options.clone()).expect("build initial search index");

    let previous = index_dir.with_extension("previous");
    let next = index_dir.with_extension("next");
    std::fs::rename(&index_dir, &previous).expect("simulate current-to-previous rename");
    std::fs::create_dir_all(&next).expect("create interrupted next generation");
    std::fs::write(next.join("partial"), b"incomplete").expect("write interrupted generation");

    let mut cancelled_options = base_options;
    cancelled_options.cancel_token = Arc::new(AtomicBool::new(true));
    let error = run_search_index_phase(cancelled_options)
        .expect_err("cancelled retry must not publish a generation");

    assert!(error.to_string().contains("cancelled by user"));
    assert!(index_dir.is_dir());
    assert!(!previous.exists());
    assert!(!next.exists());
    let result = search::SearchIndex::open(&index_dir)
        .expect("open restored index")
        .search("root", 10)
        .expect("query restored index");
    assert_eq!(result.total_count, 1);
}

#[test]
fn extensionless_linux_forensic_files_consume_budget_before_generic_text() {
    let tmp = TempDir::new().expect("create search phase fixture");
    let evidence = tmp.path().join("evidence");
    let etc = evidence.join("etc");
    let generic = evidence.join("aaa");
    std::fs::create_dir_all(&etc).expect("create etc fixture");
    std::fs::create_dir_all(&generic).expect("create generic fixture");
    let passwd = b"priority-marker:x:1:1::/nonexistent:/usr/sbin/nologin\n";
    std::fs::write(etc.join("passwd"), passwd).expect("write passwd fixture");

    let (db_path, data_source_id) = setup_source_db(&tmp);
    let connection =
        persistence_sqlite::open_existing_source(&db_path).expect("open source database");
    connection
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, encrypted)
             VALUES ('passwd', ?1, 'etc/passwd', 'passwd', 'file', ?2, NULL, 0, 0, 0, 0)",
            params![data_source_id.0, passwd.len() as u64],
        )
        .expect("insert passwd candidate");
    for index in 0..infrastructure::constants::IMPORT_TEXT_INDEX_LIMIT {
        let name = format!("generic-{index:03}.txt");
        let path = format!("aaa/{name}");
        let content = format!("generic marker {index}");
        std::fs::write(generic.join(&name), &content).expect("write generic fixture");
        connection
            .execute(
                "INSERT INTO file_entries
                 (id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, encrypted)
                 VALUES (?1, ?2, ?3, ?4, 'file', ?5, 'txt', 0, 0, 0, 0)",
                params![
                    format!("generic-{index:03}"),
                    data_source_id.0,
                    path,
                    name,
                    content.len() as u64
                ],
            )
            .expect("insert generic candidate");
    }
    connection
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, encrypted)
             VALUES
              ('missing-after-limit', ?1, 'zzz/missing-after-limit.txt',
               'missing-after-limit.txt', 'file', 24, 'txt', 0, 0, 0, 0)",
            [&data_source_id.0],
        )
        .expect("insert unreadable candidate beyond index limit");
    drop(connection);

    let index_dir = tmp.path().join("search-index");
    let stats = run_search_index_phase(SearchIndexPhaseOptions {
        case_root: tmp.path().to_path_buf(),
        db_path,
        case_id: "case-1".to_string(),
        data_source_id,
        platform: DataSourcePlatform::Linux,
        index_dir: index_dir.clone(),
        cancel_token: Arc::new(AtomicBool::new(false)),
    })
    .expect("build priority-aware search index");

    assert_eq!(
        stats.indexed_count,
        infrastructure::constants::IMPORT_TEXT_INDEX_LIMIT as u64
    );
    assert_eq!(
        stats.eligible_count,
        infrastructure::constants::IMPORT_TEXT_INDEX_LIMIT as u64 + 2
    );
    assert_eq!(stats.skipped_count, 2);
    assert_eq!(stats.failed_count, 0);
    let result = search::SearchIndex::open(&index_dir)
        .expect("open priority-aware index")
        .search("priority-marker", 10)
        .expect("search priority marker");
    assert_eq!(result.total_count, 1);
    assert!(result
        .hits
        .iter()
        .any(|hit| hit.path.replace('\\', "/").ends_with("etc/passwd")));
}
