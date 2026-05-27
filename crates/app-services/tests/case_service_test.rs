use app_services::case_service;
use tempfile::TempDir;

#[test]
fn create_case_creates_directory_structure() {
    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(tmp.path(), "test-case", Some("tester")).unwrap();
    assert_eq!(active.meta.name, "test-case");
    assert_eq!(active.meta.examiner.as_deref(), Some("tester"));

    let case_root = tmp.path().join("test-case");
    assert!(case_root.join("case.json").exists());
    assert!(case_root.join("app.db").exists());
    assert!(case_root.join("evidence").exists());
    assert!(case_root.join("exports").exists());
    assert!(case_root.join("reports").exists());
    assert!(case_root.join("indexes").exists());
    assert!(case_root.join("cache").exists());
    assert!(case_root.join("logs").exists());
}

#[test]
fn create_case_initializes_db() {
    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(tmp.path(), "db-test", None).unwrap();

    let metrics = active.with_conn(|conn| {
        let version = persistence_sqlite::runner::current_version(conn)?;
        assert_eq!(version, Some("0009_data_source_partitions".to_string()));

        let repo = persistence_sqlite::repositories::case_repo::CaseRepo::new(conn);
        let found = repo.find_by_id(&active.meta.id)?;
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "db-test");
        Ok(())
    });
    metrics.unwrap();
}

#[test]
fn open_case_reads_metadata() {
    let tmp = TempDir::new().unwrap();
    let created = case_service::create_case(tmp.path(), "open-test", Some("examiner-1")).unwrap();
    drop(created);

    let opened = case_service::open_case(&tmp.path().join("open-test")).unwrap();
    assert_eq!(opened.case_root, tmp.path().join("open-test"));
    assert_eq!(opened.meta.examiner.as_deref(), Some("examiner-1"));
}

#[test]
fn create_duplicate_name_fails() {
    let tmp = TempDir::new().unwrap();
    case_service::create_case(tmp.path(), "dup", None).unwrap();
    let result = case_service::create_case(tmp.path(), "dup", None);
    assert!(result.is_err());
}

#[test]
fn open_nonexistent_case_fails() {
    let tmp = TempDir::new().unwrap();
    let result = case_service::open_case(&tmp.path().join("does-not-exist"));
    assert!(result.is_err());
}

#[test]
fn open_case_without_json_fails() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("bad-case");
    std::fs::create_dir_all(&dir).unwrap();
    let result = case_service::open_case(&dir);
    assert!(result.is_err());
}

#[test]
fn active_case_connection_works() {
    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(tmp.path(), "conn-test", None).unwrap();

    let count = active
        .with_conn(|conn| {
            let n: i64 = conn.query_row("SELECT COUNT(*) FROM cases", [], |r| r.get(0))?;
            Ok(n)
        })
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn reopen_case_shares_no_state() {
    let tmp = TempDir::new().unwrap();
    let active1 = case_service::create_case(tmp.path(), "reopen", None).unwrap();
    let case_id = active1.meta.id.clone();
    drop(active1);

    let active2 = case_service::open_case(&tmp.path().join("reopen")).unwrap();
    assert_eq!(active2.meta.id, case_id);
}
