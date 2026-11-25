use app_services::capability_service;
use domain::{DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_sqlite::repositories::{
    capability_execution_repo::{
        CapabilityClaimOutcome, CapabilityClaimRequest, CapabilityCompletion,
        CapabilityExecutionRepo, CapabilityQueueOutcome,
    },
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    source_capability_repo::{SourceCapabilityRepo, SourceCapabilityRow},
};
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

fn state(fixture: &Fixture, kind: CapabilityKindDto) -> CapabilityStateDto {
    derive(fixture)
        .sources
        .into_iter()
        .find(|row| row.data_source_id == fixture.source.0 && row.capability == kind)
        .unwrap()
        .state
}

#[test]
fn derived_read_model_does_not_write_presentation_rows() {
    let fixture = fixture("ready", Some("windows"));
    assert_eq!(
        state(&fixture, CapabilityKindDto::Catalog),
        CapabilityStateDto::Ready
    );
    assert_eq!(
        state(&fixture, CapabilityKindDto::Search),
        CapabilityStateDto::Ready
    );
    assert_eq!(
        state(&fixture, CapabilityKindDto::Artifacts),
        CapabilityStateDto::Pending
    );
    assert_eq!(
        state(&fixture, CapabilityKindDto::Timeline),
        CapabilityStateDto::Failed
    );
    let rows = fixture
        .case
        .with_conn(|conn| SourceCapabilityRepo::new(conn).list_for_case(&fixture.case.meta.id.0))
        .unwrap();
    assert!(rows.is_empty());
}

#[test]
fn metadata_only_source_marks_artifacts_not_applicable() {
    let fixture = fixture("ready_metadata", Some("windows"));
    assert_eq!(
        state(&fixture, CapabilityKindDto::Artifacts),
        CapabilityStateDto::NotApplicable
    );
}

#[test]
fn task_confirmed_artifact_state_and_revision_are_preserved() {
    let fixture = fixture("ready", Some("windows"));
    let revision = fixture
        .case
        .with_conn(|conn| {
            let repo = CapabilityExecutionRepo::new(conn);
            let CapabilityQueueOutcome::Queued(row) =
                repo.queue_retry(&fixture.case.meta.id.0, &fixture.source.0, "artifacts")?
            else {
                panic!("artifact task must be queued")
            };
            let CapabilityClaimOutcome::Acquired(attempt) = repo.claim(CapabilityClaimRequest {
                case_id: &fixture.case.meta.id.0,
                source_id: &fixture.source.0,
                capability: "artifacts",
                revision: row.revision,
                input_fingerprint: &"a".repeat(64),
                owner: "capability-service-test",
            })?
            else {
                panic!("artifact task must be acquired")
            };
            let completed = repo
                .complete(
                    &attempt,
                    CapabilityCompletion {
                        state: "ready",
                        detail: Some("task confirmed"),
                        error_code: None,
                    },
                )?
                .unwrap();
            Ok(completed.revision)
        })
        .unwrap();
    let artifacts = derive(&fixture)
        .sources
        .into_iter()
        .find(|row| row.capability == CapabilityKindDto::Artifacts)
        .unwrap();
    assert_eq!(artifacts.state, CapabilityStateDto::Ready);
    assert!(!artifacts.derived);
    assert_eq!(artifacts.revision, revision);
    assert_eq!(artifacts.detail.as_deref(), Some("task confirmed"));
}

#[test]
fn interrupted_task_is_requeued_without_creating_other_capabilities() {
    let fixture = fixture("ready", Some("windows"));
    fixture
        .case
        .with_conn(|conn| {
            SourceCapabilityRepo::new(conn).upsert(&SourceCapabilityRow {
                case_id: fixture.case.meta.id.0.clone(),
                data_source_id: fixture.source.0.clone(),
                capability: "timeline".into(),
                state: "running".into(),
                revision: 2,
                progress_done: 0,
                progress_total: None,
                phase: None,
                detail: None,
                error_code: None,
                derived: 0,
                updated_at: chrono::Utc::now().to_rfc3339(),
            })?;
            assert_eq!(
                capability_service::requeue_running(conn, &fixture.case.meta.id).unwrap(),
                1
            );
            let rows = SourceCapabilityRepo::new(conn).list_for_case(&fixture.case.meta.id.0)?;
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].state, "queued");
            assert_eq!(rows[0].revision, 2);
            Ok(())
        })
        .unwrap();
}

#[test]
fn unknown_platform_derives_artifacts_and_facts_not_applicable() {
    let fixture = fixture("ready", None);
    assert_eq!(
        state(&fixture, CapabilityKindDto::Artifacts),
        CapabilityStateDto::NotApplicable
    );
    assert_eq!(
        state(&fixture, CapabilityKindDto::PlatformFacts),
        CapabilityStateDto::NotApplicable
    );
}
