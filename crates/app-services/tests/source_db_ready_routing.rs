use app_services::source_db::{open_ready_source_by_id, ReadySourceError};
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
}
