use app_services::datasource_service::{attach_data_source_with_storage, DataSourceError};
use app_services::import_precheck::{self, ImportSourceConfigError};
use domain::{CaseId, CaseMeta, DataSourceKind, DataSourcePlatform};
use persistence_sqlite::repositories::{case_repo::CaseRepo, datasource_repo::DataSourceRepo};

#[test]
fn domain_platform_is_persisted_with_its_canonical_value() {
    let (conn, case) = case_database();

    let attached = attach_data_source_with_storage(
        &conn,
        &case.id,
        "linux-source",
        std::path::Path::new("Z:/domain-platform/linux.E01"),
        DataSourceKind::E01,
        DataSourcePlatform::Linux,
        None,
    )
    .expect("attach Linux data source");

    let storage = DataSourceRepo::new(&conn)
        .find_storage(&attached.id)
        .expect("query storage")
        .expect("storage record");
    assert_eq!(storage.platform, DataSourcePlatform::Linux.as_storage_str());
}

#[test]
fn unknown_platform_cannot_register_a_data_source() {
    let (conn, case) = case_database();
    let error = attach_data_source_with_storage(
        &conn,
        &case.id,
        "unknown-source",
        std::path::Path::new("Z:/domain-platform/unknown.E01"),
        DataSourceKind::E01,
        DataSourcePlatform::Unknown,
        None,
    )
    .expect_err("unknown platform must fail before registration");

    assert!(matches!(
        error,
        DataSourceError::UnsupportedPlatform(ref value) if value == "unknown"
    ));
    assert!(DataSourceRepo::new(&conn)
        .find_by_case(&case.id)
        .expect("list registered sources")
        .is_empty());
}

#[test]
fn unknown_platform_fails_precheck_before_source_access() {
    let error = import_precheck::prepare_import_source_config_from_path(
        "Z:/path-that-must-not-exist/unknown.E01",
        DataSourcePlatform::Unknown,
    )
    .expect_err("unknown platform must fail before filesystem access");

    assert!(matches!(
        error,
        ImportSourceConfigError::UnsupportedPlatform
    ));
}

fn case_database() -> (rusqlite::Connection, CaseMeta) {
    let conn = persistence_sqlite::connection::open_in_memory().expect("open in-memory database");
    persistence_sqlite::runner::run_all(&conn).expect("run case migrations");
    let case = CaseMeta {
        id: CaseId("case-retired-platform".to_string()),
        name: "Retired Platform".to_string(),
        number: None,
        examiner: None,
        notes: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    CaseRepo::new(&conn).create(&case).expect("create case");
    (conn, case)
}
