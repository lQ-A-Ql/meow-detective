use app_services::datasource_service::{attach_data_source_with_storage, DataSourceError};
use domain::{CaseId, CaseMeta, DataSourceKind};
use persistence_sqlite::repositories::{case_repo::CaseRepo, datasource_repo::DataSourceRepo};
use transport::commands::ImportTargetPlatformDto;

#[test]
fn retired_platform_is_rejected_before_data_source_registration() {
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

    let error = attach_data_source_with_storage(
        &conn,
        &case.id,
        "retired-source",
        std::path::Path::new("Z:/path-that-must-not-be-read/retired.E01"),
        DataSourceKind::E01,
        Some(ImportTargetPlatformDto::Unsupported),
        None,
    )
    .expect_err("retired platform must fail closed");

    assert!(matches!(error, DataSourceError::Unsupported(_)));
    assert!(DataSourceRepo::new(&conn)
        .find_by_case(&case.id)
        .expect("query data sources")
        .is_empty());
}
