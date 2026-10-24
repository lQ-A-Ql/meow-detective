use app_services::source_db::{
    open_ready_source_by_id, open_ready_source_read_only_by_id, ReadySourceError,
};
use domain::{
    CaseId, CaseMeta, DataSource, DataSourceId, DataSourceKind, DataSourcePlatform,
    DataSourceProvenance,
};
use persistence_sqlite::repositories::{
    case_repo::CaseRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
};

fn create_case(conn: &rusqlite::Connection, case_id: &str) -> CaseId {
    let case_id = CaseId(case_id.to_string());
    CaseRepo::new(conn)
        .create(&CaseMeta {
            id: case_id.clone(),
            name: case_id.0.clone(),
            number: None,
            examiner: None,
            notes: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .expect("create case");
    case_id
}

fn register_source(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    source_id: &str,
    platform: &str,
    import_state: &str,
) -> DataSourceId {
    let source = DataSource {
        id: DataSourceId(source_id.to_string()),
        name: source_id.to_string(),
        kind: DataSourceKind::E01,
        source_path: case_root.join(format!("{source_id}.E01")),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(source_id, Some(platform), None);
    storage.import_state = import_state.to_string();
    DataSourceRepo::new(conn)
        .insert_with_storage(case_id, &source, &storage)
        .expect("register source");
    drop(
        app_services::source_db::open_source_db(case_root, &source.id)
            .expect("create source database"),
    );
    source.id
}

#[test]
fn ready_route_rejects_every_unfinished_bulk_state_and_orphan_manifest() {
    use persistence_sqlite::repositories::source_bulk_load_repo::{
        BulkLoadState, SourceBulkLoadRepo as Bulk,
    };
    let root = tempfile::tempdir().unwrap();
    let case = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&case).unwrap();
    let id = create_case(&case, "bulk-case");
    let source = register_source(&case, root.path(), &id, "bulk-source", "windows", "ready");
    let path =
        app_services::source_db::registered_source_db_path(&case, root.path(), &source).unwrap();
    let db = persistence_sqlite::open_existing_source(&path).unwrap();
    Bulk::begin(&db, "unfinished").unwrap();
    for state in [
        BulkLoadState::Active,
        BulkLoadState::Rebuilding,
        BulkLoadState::Ready,
    ] {
        Bulk::set_state(&db, state, "unfinished").unwrap();
        assert!(open_ready_source_by_id(&case, root.path(), &id, &source).is_err());
        assert!(app_services::source_db::open_ready_source_read_only_by_id(
            &case,
            root.path(),
            &id,
            &source
        )
        .is_err());
    }
    Bulk::recover(&db).unwrap();
    assert!(open_ready_source_by_id(&case, root.path(), &id, &source).is_ok());
    db.execute(
        "INSERT INTO source_meta(key,value) VALUES('bulk_load.indexes','[]')",
        [],
    )
    .unwrap();
    assert!(open_ready_source_by_id(&case, root.path(), &id, &source).is_err());
    assert!(Bulk::recover(&db).is_err());
}

#[test]
fn ready_source_route_enforces_case_state_and_platform_before_opening() {
    let case_root = tempfile::TempDir::new().expect("case root");
    let case_conn = persistence_sqlite::open_in_memory().expect("control database");
    persistence_sqlite::runner::run_all(&case_conn).expect("control migrations");
    let current_case = create_case(&case_conn, "case-current");
    let other_case = create_case(&case_conn, "case-other");

    let pending = register_source(
        &case_conn,
        case_root.path(),
        &current_case,
        "source-pending",
        "windows",
        "pending",
    );
    assert!(matches!(
        open_ready_source_by_id(&case_conn, case_root.path(), &current_case, &pending),
        Err(ReadySourceError::NotReady { .. })
    ));

    let unsupported = register_source(
        &case_conn,
        case_root.path(),
        &current_case,
        "source-unsupported",
        "unknown",
        "ready",
    );
    assert!(matches!(
        open_ready_source_by_id(&case_conn, case_root.path(), &current_case, &unsupported),
        Err(ReadySourceError::UnsupportedPlatform { .. })
    ));

    let other_source = register_source(
        &case_conn,
        case_root.path(),
        &other_case,
        "source-other-case",
        "linux",
        "ready",
    );
    assert!(matches!(
        open_ready_source_by_id(&case_conn, case_root.path(), &current_case, &other_source,),
        Err(ReadySourceError::NotFound { .. })
    ));

    let ready = register_source(
        &case_conn,
        case_root.path(),
        &current_case,
        "source-ready",
        "linux",
        "ready",
    );
    let opened = open_ready_source_by_id(&case_conn, case_root.path(), &current_case, &ready)
        .expect("open ready source");
    assert_eq!(opened.data_source_id, ready);
    assert_eq!(opened.platform, DataSourcePlatform::Linux);

    let read_only =
        open_ready_source_read_only_by_id(&case_conn, case_root.path(), &current_case, &ready)
            .expect("open ready source read-only");
    assert_eq!(
        read_only
            .connection
            .pragma_query_value(None, "query_only", |row| row.get::<_, i64>(0))
            .expect("query_only pragma"),
        1
    );
    assert_eq!(
        read_only
            .connection
            .execute(
                "UPDATE source_meta SET value='forbidden' WHERE key='source_id'",
                [],
            )
            .expect_err("source read route must reject writes")
            .sqlite_error_code(),
        Some(rusqlite::ErrorCode::ReadOnly)
    );
}
