use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use domain::DataSourcePlatform;
use rusqlite::params;
use tempfile::TempDir;

use super::setup_case_db;
use crate::import_analysis::{run_search_index_phase, SearchIndexPhaseOptions};

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

    let (db_path, data_source_id) = setup_case_db(&tmp);
    let connection = persistence_sqlite::open_or_create(&db_path).expect("open source database");
    connection
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system)
             VALUES
             ('passwd', ?1, 'etc/passwd', 'passwd', 'file', ?2, NULL, 0, 0, 0),
             ('empty', ?1, 'etc/empty.txt', 'empty.txt', 'file', 0, 'txt', 0, 0, 0)",
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
fn failed_search_generation_preserves_the_previous_complete_index() {
    let tmp = TempDir::new().expect("create search phase fixture");
    let evidence = tmp.path().join("evidence");
    let etc = evidence.join("etc");
    std::fs::create_dir_all(&etc).expect("create etc fixture");
    let passwd =
        b"root:x:0:0:root:/root:/bin/bash\ndaemon:x:1:1:daemon:/usr/sbin:/usr/sbin/nologin\n";
    std::fs::write(etc.join("passwd"), passwd).expect("write passwd fixture");

    let (db_path, data_source_id) = setup_case_db(&tmp);
    let connection = persistence_sqlite::open_or_create(&db_path).expect("open source database");
    connection
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system)
             VALUES ('passwd', ?1, 'etc/passwd', 'passwd', 'file', ?2, NULL, 0, 0, 0)",
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

    let connection = persistence_sqlite::open_or_create(&db_path).expect("reopen source database");
    connection
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system)
             VALUES ('missing', ?1, 'etc/missing.txt', 'missing.txt', 'file', 12, 'txt', 0, 0, 0)",
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
fn extensionless_linux_forensic_files_consume_budget_before_generic_text() {
    let tmp = TempDir::new().expect("create search phase fixture");
    let evidence = tmp.path().join("evidence");
    let etc = evidence.join("etc");
    let generic = evidence.join("aaa");
    std::fs::create_dir_all(&etc).expect("create etc fixture");
    std::fs::create_dir_all(&generic).expect("create generic fixture");
    let passwd = b"priority-marker:x:1:1::/nonexistent:/usr/sbin/nologin\n";
    std::fs::write(etc.join("passwd"), passwd).expect("write passwd fixture");

    let (db_path, data_source_id) = setup_case_db(&tmp);
    let connection = persistence_sqlite::open_or_create(&db_path).expect("open source database");
    connection
        .execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system)
             VALUES ('passwd', ?1, 'etc/passwd', 'passwd', 'file', ?2, NULL, 0, 0, 0)",
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
                 (id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system)
                 VALUES (?1, ?2, ?3, ?4, 'file', ?5, 'txt', 0, 0, 0)",
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
    assert_eq!(stats.skipped_count, 1);
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
