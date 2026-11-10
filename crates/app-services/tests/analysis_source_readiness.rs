use std::path::Path;

use app_services::analysis_service::{classify_source_files, AnalysisServiceError};
use domain::{CaseId, CaseMeta, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_sqlite::repositories::{
    case_repo::CaseRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
};
use transport::{ErrorCategory, ServiceErrorCategory};

fn register_source(
    case_conn: &rusqlite::Connection,
    case_id: &CaseId,
    case_root: &Path,
    source_id: &str,
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
    let mut storage = DataSourceStorage::source_db(source_id, Some("windows"), None);
    storage.import_state = import_state.to_string();
    DataSourceRepo::new(case_conn)
        .insert_with_storage(case_id, &source, &storage)
        .expect("register source");

    let source_conn =
        app_services::source_db::open_source_db(case_root, &source.id).expect("create source db");
    let store = persistence_sqlite::source_fjall::open_store_for_write(&source_conn)
        .expect("open source store")
        .expect("configured source store");
    persistence_fjall::seal_source_generation(&store, &source.id.0, "fixture", &"a".repeat(64))
        .expect("seal source generation");
    let key = persistence_fjall::encode_key(
        "projection",
        &[persistence_fjall::KeyPart::Text(source.id.0.clone())],
    )
    .expect("encode projection key");
    let marker = persistence_fjall::VersionedEnvelope::new(
        "source_projection_complete",
        1,
        format!("projection:{}", source.id.0),
        &source.id.0,
        1,
        source.id.0.as_bytes().to_vec(),
    );
    store
        .write_envelope(
            persistence_fjall::KeyspaceName::Meta,
            key,
            &marker,
            persistence_fjall::PersistMode::SyncAll,
        )
        .expect("mark source projection complete");
    source.id
}

#[test]
fn analysis_use_cases_reject_non_ready_sources_and_open_ready_sources() {
    let case_root = tempfile::TempDir::new().expect("case root");
    let case_conn = persistence_sqlite::open_in_memory().expect("open case database");
    persistence_sqlite::runner::run_all(&case_conn).expect("run case migrations");
    let case_id = CaseId("case-analysis-readiness".to_string());
    CaseRepo::new(&case_conn)
        .create(&CaseMeta {
            id: case_id.clone(),
            name: "Analysis readiness".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .expect("create case");

    let pending = register_source(
        &case_conn,
        &case_id,
        case_root.path(),
        "source-pending",
        "pending",
    );
    let error = classify_source_files(&case_conn, case_root.path(), &case_id, &pending, 10)
        .expect_err("pending source must fail closed");
    assert!(matches!(error, AnalysisServiceError::InvalidInput(_)));
    assert!(matches!(error.category(), ErrorCategory::Validation));

    let ready = register_source(
        &case_conn,
        &case_id,
        case_root.path(),
        "source-ready",
        "ready",
    );
    let classifications = classify_source_files(&case_conn, case_root.path(), &case_id, &ready, 10)
        .expect("ready source should open");
    assert!(classifications.is_empty());
}
