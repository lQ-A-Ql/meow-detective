use app_services::capability_service;
use domain::{DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};
use persistence_sqlite::repositories::source_capability_repo::SourceCapabilityRepo;
use transport::dto::{CapabilityKindDto, CapabilityStateDto};

fn confirm_artifacts(
    conn: &rusqlite::Connection,
    case: &domain::CaseId,
    source: &str,
) -> persistence_sqlite::DbResult<()> {
    use persistence_sqlite::repositories::capability_execution_repo::{
        CapabilityClaimOutcome, CapabilityClaimRequest, CapabilityCompletion,
        CapabilityExecutionRepo, CapabilityQueueOutcome,
    };
    let repo = CapabilityExecutionRepo::new(conn);
    let CapabilityQueueOutcome::Queued(row) = repo.queue_retry(&case.0, source, "artifacts")?
    else {
        panic!("artifact request must be queued")
    };
    let CapabilityClaimOutcome::Acquired(attempt) = repo.claim(CapabilityClaimRequest {
        case_id: &case.0,
        source_id: source,
        capability: "artifacts",
        revision: row.revision,
        input_fingerprint: &"a".repeat(64),
        owner: "capability-service-test",
    })?
    else {
        panic!("artifact request must be acquired")
    };
    assert!(repo
        .complete(
            &attempt,
            CapabilityCompletion {
                state: "ready",
                detail: Some("task confirmed"),
                error_code: None
            }
        )?
        .is_some());
    Ok(())
}

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
    // This fixture has no source store, so availability cannot be confirmed.
    assert_eq!(
        find(CapabilityKindDto::Timeline).state,
        CapabilityStateDto::Failed
    );
    assert!(find(CapabilityKindDto::Timeline).derived);
    assert_eq!(
        find(CapabilityKindDto::Graph).state,
        CapabilityStateDto::Failed
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
        .with_conn(|conn| confirm_artifacts(conn, &fixture.case.meta.id, &fixture.source.0))
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
    assert_eq!(artifacts.revision, 2);
}

#[test]
fn backfill_is_idempotent_and_preserves_task_confirmed_rows() {
    let fixture = fixture("ready", Some("windows"));
    let case_id = fixture.case.meta.id.0.clone();
    let source_id = fixture.source.0.clone();
    // First backfill writes the four cheap derived rows.
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
    let before = fixture
        .case
        .with_conn(|conn| SourceCapabilityRepo::new(conn).list_for_case(&case_id))
        .unwrap();
    assert_eq!(before.len(), 4);

    // A task-confirmed artifacts row replaces the derived placeholder.
    fixture
        .case
        .with_conn(|conn| confirm_artifacts(conn, &fixture.case.meta.id, &source_id))
        .unwrap();

    // A second backfill must not add rows or overwrite the task-confirmed row.
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
        .with_conn(|conn| SourceCapabilityRepo::new(conn).list_for_case(&case_id))
        .unwrap();
    assert_eq!(rows.len(), 4, "repeated backfill must not add rows");
    let artifacts = rows
        .iter()
        .find(|row| row.capability == "artifacts")
        .expect("artifacts row");
    assert_eq!(artifacts.state, "ready");
    assert_eq!(artifacts.derived, 0);
    assert_eq!(artifacts.detail.as_deref(), Some("task confirmed"));
    assert_eq!(artifacts.revision, 2);
    // Derived placeholder rows remain untouched.
    for kind in ["catalog", "search", "platformFacts"] {
        let row = rows
            .iter()
            .find(|entry| entry.capability == kind)
            .expect("derived row");
        assert_eq!(row.derived, 1, "{kind} row must stay derived");
    }
}

#[test]
fn requeue_running_recovers_running_without_losing_rows() {
    let fixture = fixture("ready", Some("windows"));
    let case_id = fixture.case.meta.id.0.clone();
    let source_id = fixture.source.0.clone();
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
    // Leave a stale running timeline row behind from a dead process; the
    // backfilled catalog row stays terminal ready.
    fixture
        .case
        .with_conn(|conn| -> Result<(), persistence_sqlite::DbError> {
            SourceCapabilityRepo::new(conn).upsert(
                &persistence_sqlite::repositories::source_capability_repo::SourceCapabilityRow {
                    case_id: case_id.clone(),
                    data_source_id: source_id.clone(),
                    capability: "timeline".to_string(),
                    state: "running".to_string(),
                    revision: 2,
                    progress_done: 0,
                    progress_total: None,
                    phase: None,
                    detail: None,
                    error_code: None,
                    derived: 0,
                    updated_at: chrono::Utc::now().to_rfc3339(),
                },
            )?;
            Ok(())
        })
        .unwrap();
    let total_before = fixture
        .case
        .with_conn(|conn| SourceCapabilityRepo::new(conn).list_for_case(&case_id))
        .unwrap()
        .len();

    let recovered = fixture
        .case
        .with_conn(|conn| {
            capability_service::requeue_running(conn, &fixture.case.meta.id)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })
        .unwrap();
    assert_eq!(recovered, 1);

    fixture
        .case
        .with_conn(|conn| -> Result<(), persistence_sqlite::DbError> {
            let repo = SourceCapabilityRepo::new(conn);
            let rows = repo.list_for_case(&case_id)?;
            assert_eq!(rows.len(), total_before, "requeue must not drop rows");
            let pending = repo.list_pending(&case_id)?;
            // The backfilled artifacts placeholder is still pending; the stale
            // running timeline row recovered to queued alongside it.
            assert_eq!(pending.len(), 2);
            let timeline = pending
                .iter()
                .find(|row| row.capability == "timeline")
                .expect("timeline row");
            assert_eq!(timeline.state, "queued");
            assert_eq!(timeline.revision, 2, "revision preserved across requeue");
            let artifacts = pending
                .iter()
                .find(|row| row.capability == "artifacts")
                .expect("artifacts row");
            assert_eq!(artifacts.state, "pending");
            // Terminal ready rows are never eligible for re-enqueue.
            for kind in ["catalog", "search", "platformFacts"] {
                assert!(!pending.iter().any(|row| row.capability == kind));
            }
            let catalog = rows
                .iter()
                .find(|row| row.capability == "catalog")
                .expect("catalog row");
            assert_eq!(catalog.state, "ready");
            Ok(())
        })
        .unwrap();
}

#[test]
fn unknown_platform_derives_artifacts_and_facts_not_applicable() {
    let fixture = fixture("ready", None);
    let matrix = derive(&fixture);
    let find = |kind: CapabilityKindDto| {
        matrix
            .sources
            .iter()
            .find(|entry| entry.capability == kind && entry.data_source_id == fixture.source.0)
            .expect("capability entry")
    };
    assert_eq!(
        find(CapabilityKindDto::Artifacts).state,
        CapabilityStateDto::NotApplicable
    );
    assert_eq!(
        find(CapabilityKindDto::PlatformFacts).state,
        CapabilityStateDto::NotApplicable
    );
    assert_eq!(
        find(CapabilityKindDto::Catalog).state,
        CapabilityStateDto::Ready
    );
}
