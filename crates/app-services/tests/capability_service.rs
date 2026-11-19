use app_services::capability_service;
use domain::{DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};
use persistence_sqlite::repositories::source_capability_repo::SourceCapabilityRepo;
use transport::dto::{CapabilityKindDto, CapabilityStateDto};

struct Fixture {
    _root: tempfile::TempDir,
    case: app_services::active_case::ActiveCase,
    source: DataSourceId,
}

fn fixture(import_state: &str, platform: Option<&str>) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let case =
        app_services::case_service::create_case(root.path(), "Capability service", None).unwrap();
    let source = DataSourceId("capability-source".into());
    let data_source = DataSource {
        id: source.clone(),
        name: "Capability source".into(),
        kind: DataSourceKind::E01,
        source_path: root.path().join("source.E01"),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(&source.0, platform, None);
    storage.import_state = import_state.into();
    case.with_conn(|conn| {
        DataSourceRepo::new(conn).insert_with_storage(&case.meta.id, &data_source, &storage)
    })
    .unwrap();
    Fixture {
        _root: root,
        case,
        source,
    }
}

fn derive(fixture: &Fixture) -> transport::dto::CapabilityMatrixDto {
    fixture
        .case
        .with_conn(|conn| {
            capability_service::derive_capability_matrix(
                conn,
                &fixture.case.case_root,
                &fixture.case.meta.id,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .unwrap()
}

#[test]
fn backfill_writes_cheap_rows_and_derive_maps_ready_catalog() {
    let fixture = fixture("ready", Some("windows"));
    fixture
        .case
        .with_conn(|conn| {
            capability_service::backfill_case_capabilities(
                conn,
                &fixture.case.case_root,
                &fixture.case.meta.id,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .unwrap();

    let rows = fixture
        .case
        .with_conn(|conn| SourceCapabilityRepo::new(conn).list_for_case(&fixture.case.meta.id.0))
        .unwrap();
    assert_eq!(rows.len(), 4);
    assert!(rows.iter().all(|row| row.derived == 1));

    let matrix = derive(&fixture);
    let find = |kind: CapabilityKindDto| {
        matrix
            .sources
            .iter()
            .find(|entry| entry.capability == kind && entry.data_source_id == fixture.source.0)
            .expect("capability entry")
    };

    assert_eq!(
        find(CapabilityKindDto::Catalog).state,
        CapabilityStateDto::Ready
    );
    let search = find(CapabilityKindDto::Search);
    assert_eq!(search.state, CapabilityStateDto::Ready);
    assert_eq!(search.detail.as_deref(), Some("derived from catalog"));
    assert!(search.derived);
    assert_eq!(
        find(CapabilityKindDto::PlatformFacts).state,
        CapabilityStateDto::Ready
    );
    assert_eq!(
        find(CapabilityKindDto::Artifacts).state,
        CapabilityStateDto::Pending
    );
    // No source DB marker in this fixture: legacy missing marker derives ready.
    assert_eq!(
        find(CapabilityKindDto::Timeline).state,
        CapabilityStateDto::Ready
    );
    assert!(find(CapabilityKindDto::Timeline).derived);
    assert_eq!(
        find(CapabilityKindDto::Graph).state,
        CapabilityStateDto::Ready
    );
}

#[test]
fn metadata_only_source_marks_artifacts_not_applicable() {
    let fixture = fixture("ready_metadata", Some("windows"));
    let matrix = derive(&fixture);
    let artifacts = matrix
        .sources
        .iter()
        .find(|entry| {
            entry.capability == CapabilityKindDto::Artifacts
                && entry.data_source_id == fixture.source.0
        })
        .expect("artifacts entry");
    assert_eq!(artifacts.state, CapabilityStateDto::NotApplicable);
}

#[test]
fn task_confirmed_artifact_row_is_preserved_and_revision_bumps() {
    let fixture = fixture("ready", Some("windows"));
    fixture
        .case
        .with_conn(|conn| {
            capability_service::backfill_case_capabilities(
                conn,
                &fixture.case.case_root,
                &fixture.case.meta.id,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .unwrap();
    fixture
        .case
        .with_conn(|conn| {
            capability_service::mark_capability_state(
                conn,
                &fixture.case.meta.id,
                &fixture.source.0,
                CapabilityKindDto::Artifacts,
                CapabilityStateDto::Ready,
                None,
                Some("task confirmed".into()),
                None,
                false,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .unwrap();

    let matrix = derive(&fixture);
    let artifacts = matrix
        .sources
        .iter()
        .find(|entry| {
            entry.capability == CapabilityKindDto::Artifacts
                && entry.data_source_id == fixture.source.0
        })
        .expect("artifacts entry");
    assert_eq!(artifacts.state, CapabilityStateDto::Ready);
    assert!(!artifacts.derived);
    assert_eq!(artifacts.detail.as_deref(), Some("task confirmed"));
    assert_eq!(artifacts.revision, 1);
}

#[test]
fn try_claim_flips_queued_once_and_requeue_running_recovers() {
    let fixture = fixture("ready", Some("windows"));
    let case_id = fixture.case.meta.id.0.clone();
    let source_id = fixture.source.0.clone();
    fixture
        .case
        .with_conn(|conn| -> Result<(), persistence_sqlite::DbError> {
            let repo = SourceCapabilityRepo::new(conn);
            let row =
                persistence_sqlite::repositories::source_capability_repo::SourceCapabilityRow {
                    case_id: case_id.clone(),
                    data_source_id: source_id.clone(),
                    capability: "timeline".to_string(),
                    state: "queued".to_string(),
                    revision: 0,
                    progress_done: 0,
                    progress_total: None,
                    phase: None,
                    detail: None,
                    error_code: None,
                    derived: 0,
                    updated_at: chrono::Utc::now().to_rfc3339(),
                };
            repo.insert_if_missing(&row)?;
            // First claim wins; a second claim loses because state moved to running.
            assert!(repo.try_claim(&case_id, &source_id, "timeline")?);
            assert!(!repo.try_claim(&case_id, &source_id, "timeline")?);
            // A finished row can never be claimed.
            repo.update_state(&case_id, &source_id, "timeline", "ready", None, None, 0)?;
            assert!(!repo.try_claim(&case_id, &source_id, "timeline")?);
            Ok(())
        })
        .unwrap();

    fixture
        .case
        .with_conn(|conn| -> Result<(), persistence_sqlite::DbError> {
            let repo = SourceCapabilityRepo::new(conn);
            // Leave another row running to be recovered by restart reconciliation.
            let row =
                persistence_sqlite::repositories::source_capability_repo::SourceCapabilityRow {
                    case_id: case_id.clone(),
                    data_source_id: source_id.clone(),
                    capability: "artifacts".to_string(),
                    state: "running".to_string(),
                    revision: 0,
                    progress_done: 0,
                    progress_total: None,
                    phase: None,
                    detail: None,
                    error_code: None,
                    derived: 0,
                    updated_at: chrono::Utc::now().to_rfc3339(),
                };
            repo.upsert(&row)?;
            let recovered = capability_service::requeue_running(conn, &fixture.case.meta.id)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            assert_eq!(recovered, 1);
            let pending = repo.list_pending(&case_id)?;
            assert!(pending
                .iter()
                .any(|entry| entry.capability == "artifacts" && entry.state == "queued"));
            assert!(!pending.iter().any(|entry| entry.capability == "timeline"));
            Ok(())
        })
        .unwrap();
}
