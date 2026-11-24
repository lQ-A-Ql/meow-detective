use app_services::import_analysis::ImportAnalysisMode;
use app_services::import_job_coordinator::{
    run_single_import_with_mode, SingleImportMode, SingleImportRequest,
};
use app_services::timeline_service::{
    get_timeline_projection_status, rebuild_timeline_projection, TimelineProjectionPolicy,
};
use app_services::{
    capability_service, case_service, post_import_service, source_db,
    task_runtime::{TaskManager, TaskPriority, TaskScope},
};
use domain::{DataSourceId, DataSourcePlatform};
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};
use std::time::Duration;
use transport::dto::{CapabilityKindDto, CapabilityStateDto, TimelineProjectionStateDto};

struct ImportedCase {
    _root: tempfile::TempDir,
    case: app_services::active_case::ActiveCase,
    source: DataSourceId,
}

#[derive(Default)]
struct ProgressEvents(Mutex<Vec<transport::events::EventEnvelope<serde_json::Value>>>);

impl post_import_service::ApplicationEventSink for ProgressEvents {
    fn emit(
        &self,
        event: &transport::events::EventEnvelope<serde_json::Value>,
    ) -> std::io::Result<()> {
        self.0.lock().unwrap().push(event.clone());
        Ok(())
    }
}

fn import_directory() -> ImportedCase {
    import_directory_with_entries(
        DataSourcePlatform::Windows,
        &[("document.txt", "investigation text\n")],
    )
}

fn import_directory_with_entries(
    platform: DataSourcePlatform,
    entries: &[(&str, &str)],
) -> ImportedCase {
    let root = tempfile::tempdir().unwrap();
    let evidence = root.path().join("evidence");
    std::fs::create_dir_all(&evidence).unwrap();
    for (relative, contents) in entries {
        let path = evidence.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    let case = case_service::create_case(root.path(), "capability lifecycle", None).unwrap();
    let source = case
        .with_conn(|conn| {
            let config = app_services::import_precheck::prepare_import_source_config_from_path(
                &evidence.to_string_lossy(),
                platform,
            )
            .unwrap();
            let job = JobRepo::new(conn).create(&case.meta.id.0, "directory import")?;
            run_single_import_with_mode(
                SingleImportRequest {
                    connection: conn,
                    case_id: &case.meta.id,
                    case_root: &case.case_root,
                    job_id: &job,
                    import_config: config,
                    max_import_workers: Some(1),
                    max_analysis_workers: Some(1),
                    analysis_mode: ImportAnalysisMode::MetadataOnly,
                    cancel_token: &Arc::new(AtomicBool::new(false)),
                    event_sink: None,
                },
                SingleImportMode::Detached,
            )
            .unwrap();
            Ok(DataSourceRepo::new(conn)
                .find_by_case(&case.meta.id)?
                .remove(0)
                .id)
        })
        .unwrap();
    ImportedCase {
        _root: root,
        case,
        source,
    }
}

fn runtime() -> post_import_service::PostImportRuntime {
    post_import_service::PostImportRuntime {
        tasks: Arc::new(TaskManager::new()),
        bitlocker: Arc::new(app_services::bitlocker_runtime::BitLockerUnlockRegistry::default()),
        events: None,
    }
}

fn state(fixture: &ImportedCase, kind: CapabilityKindDto) -> CapabilityStateDto {
    let conn = fixture.case.connection().unwrap();
    capability_service::derive_capability_matrix(
        &conn,
        &fixture.case.case_root,
        &fixture.case.meta.id,
    )
    .unwrap()
    .sources
    .into_iter()
    .find(|row| row.capability == kind && row.data_source_id == fixture.source.0)
    .unwrap()
    .state
}

#[test]
fn detached_catalog_is_ready_while_timeline_and_artifacts_are_queued() {
    let fixture = import_directory();
    assert_eq!(
        state(&fixture, CapabilityKindDto::Catalog),
        CapabilityStateDto::Ready
    );
    assert_eq!(
        state(&fixture, CapabilityKindDto::Timeline),
        CapabilityStateDto::Queued
    );
    assert_eq!(
        state(&fixture, CapabilityKindDto::Graph),
        CapabilityStateDto::Queued
    );
    assert_eq!(
        state(&fixture, CapabilityKindDto::Artifacts),
        CapabilityStateDto::Queued
    );
    let conn = fixture.case.connection().unwrap();
    let status =
        get_timeline_projection_status(&conn, &fixture.case.case_root, &fixture.case.meta.id)
            .unwrap();
    assert_eq!(status.state, TimelineProjectionStateDto::Deferred);
}

#[test]
fn scoped_executor_completes_both_capabilities_after_real_directory_import() {
    let fixture = import_directory();
    let mut runtime = runtime();
    let events = Arc::new(ProgressEvents::default());
    runtime.events = Some(events.clone());
    assert_eq!(
        post_import_service::enqueue_case_pending(
            &runtime,
            &fixture.case.meta.id,
            &fixture.case.case_root
        )
        .unwrap(),
        2
    );
    for (_, result) in runtime.tasks.wait_all(Duration::from_secs(20)) {
        result.unwrap();
    }
    assert_eq!(
        runtime.tasks.task_count_for_case(&fixture.case.meta.id.0),
        0
    );
    assert_eq!(
        state(&fixture, CapabilityKindDto::Timeline),
        CapabilityStateDto::Ready
    );
    assert_eq!(
        state(&fixture, CapabilityKindDto::Graph),
        CapabilityStateDto::Ready
    );
    assert_eq!(
        state(&fixture, CapabilityKindDto::Artifacts),
        CapabilityStateDto::Ready
    );
    let conn = fixture.case.connection().unwrap();
    assert!(
        app_services::timeline_service::count_timeline_events_for_case(
            &conn,
            &fixture.case.case_root,
            &fixture.case.meta.id
        )
        .unwrap()
            > 0
    );
    let progress = events
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|event| event.payload.get("stages").is_some())
        .map(|event| {
            serde_json::from_value::<transport::dto::TaskProgressDto>(event.payload.clone())
                .unwrap()
        })
        .collect::<Vec<_>>();
    for kind in [
        transport::dto::TaskProgressKindDto::Analysis,
        transport::dto::TaskProgressKindDto::Timeline,
    ] {
        let completed = progress
            .iter()
            .filter(|event| event.kind == kind)
            .last()
            .unwrap();
        assert_eq!(
            completed.state,
            transport::dto::TaskProgressStateDto::Completed
        );
        assert_eq!(completed.progress, 100);
        if kind == transport::dto::TaskProgressKindDto::Timeline {
            assert!(progress
                .iter()
                .any(|event| event.kind == kind && event.work.is_some()));
        }
        assert_eq!(
            completed.data_source_id.as_deref(),
            Some(fixture.source.0.as_str())
        );
    }
}

#[test]
fn completed_capabilities_refresh_the_case_graph_publication() {
    let fixture = import_directory();
    let before = fixture
        .case
        .with_conn(|conn| {
            app_services::case_projection_service::rebuild_case_projections(
                conn,
                &fixture.case.case_root,
                &fixture.case.meta.id.0,
                &AtomicBool::new(false),
            )
            .unwrap();
            Ok(
                app_services::graph_service::get_graph_snapshot_for_case_read_only(
                    conn,
                    &fixture.case.case_root,
                    &fixture.case.meta.id.0,
                )
                .unwrap(),
            )
        })
        .unwrap();
    let runtime = runtime();
    post_import_service::enqueue_case_pending(
        &runtime,
        &fixture.case.meta.id,
        &fixture.case.case_root,
    )
    .unwrap();
    for (_, result) in runtime.tasks.wait_all(Duration::from_secs(20)) {
        result.unwrap();
    }
    let after = fixture
        .case
        .with_conn(|conn| {
            Ok(
                app_services::graph_service::get_graph_snapshot_for_case_read_only(
                    conn,
                    &fixture.case.case_root,
                    &fixture.case.meta.id.0,
                )
                .unwrap(),
            )
        })
        .unwrap();
    assert_eq!(after.projection_status, "ready");
    assert!(after.total_nodes > before.total_nodes);
}

#[test]
fn case_projection_requests_coalesce_until_source_work_finishes() {
    use app_services::case_projection_service::{schedule_case_projection, CaseProjectionRuntime};
    let fixture = import_directory();
    let runtime = runtime();
    let root = fixture.case.case_root.clone();
    let source = fixture.source.clone();
    let case = fixture.case.meta.id.clone();
    let (entered, observed) = mpsc::channel();
    let (release, wait) = mpsc::sync_channel(0);
    runtime
        .tasks
        .spawn_scoped_parallel(
            "source-graph-writer".into(),
            TaskScope::data_source(
                &case.0,
                &source.0,
                format!(
                    "{}source-graph",
                    app_services::post_import_service::CAPABILITY_TASK_GROUP_PREFIX
                ),
            ),
            TaskPriority::Background,
            Arc::new(AtomicBool::new(false)),
            move || {
                entered.send(()).unwrap();
                wait.recv().unwrap();
                let conn = source_db::open_source_db(&root, &source).unwrap();
                persistence_sqlite::repositories::graph_repo::GraphRepo::new(&conn)
                    .insert_nodes_batch(&[domain::GraphNode {
                        id: "published-source-node".into(),
                        case_id: case.0,
                        node_type: domain::NodeType::Artifact,
                        label: "Source result".into(),
                        summary: "Published source graph".into(),
                        tags: Vec::new(),
                        created_at: chrono::Utc::now().to_rfc3339(),
                    }])
                    .unwrap();
                Ok(())
            },
        )
        .unwrap();
    observed.recv_timeout(Duration::from_secs(2)).unwrap();
    let projections = CaseProjectionRuntime {
        tasks: Arc::clone(&runtime.tasks),
        events: None,
    };
    assert!(schedule_case_projection(
        &projections,
        &fixture.case.meta.id.0,
        &fixture.case.case_root
    )
    .unwrap()
    .is_some());
    assert!(schedule_case_projection(
        &projections,
        &fixture.case.meta.id.0,
        &fixture.case.case_root
    )
    .unwrap()
    .is_none());
    release.send(()).unwrap();
    for (_, result) in runtime.tasks.wait_all(Duration::from_secs(20)) {
        result.unwrap();
    }
    fixture
        .case
        .with_conn(|conn| {
            let jobs = JobRepo::new(conn).list_recent(20)?;
            assert_eq!(
                jobs.iter()
                    .filter(|job| job.kind == "Case projections")
                    .count(),
                1
            );
            let graph = app_services::graph_service::get_graph_snapshot_for_case_read_only(
                conn,
                &fixture.case.case_root,
                &fixture.case.meta.id.0,
            )
            .unwrap();
            assert_eq!(graph.projection_status, "ready");
            assert!(graph.total_nodes > 0);
            Ok(())
        })
        .unwrap();
}

#[test]
fn rejected_case_projection_registration_leaves_a_terminal_job() {
    let fixture = import_directory();
    let runtime = runtime();
    runtime
        .tasks
        .retire_case_and_drain(&fixture.case.meta.id.0, Duration::from_secs(1));
    let projections = app_services::case_projection_service::CaseProjectionRuntime {
        tasks: runtime.tasks,
        events: None,
    };
    assert!(
        app_services::case_projection_service::schedule_case_projection(
            &projections,
            &fixture.case.meta.id.0,
            &fixture.case.case_root,
        )
        .is_err()
    );
    fixture
        .case
        .with_conn(|conn| {
            let job = JobRepo::new(conn)
                .list_recent(20)?
                .into_iter()
                .find(|job| job.kind == "Case projections")
                .unwrap();
            assert_eq!(job.status, "failed");
            Ok(())
        })
        .unwrap();
}

#[test]
fn requested_rebuild_uses_the_same_capability_executor_and_read_model() {
    let fixture = import_directory();
    let runtime = runtime();
    for _ in 0..2 {
        let status = post_import_service::rebuild_timeline(
            &runtime,
            &fixture.case.meta.id,
            &fixture.case.case_root,
            Some(&fixture.source.0),
        )
        .unwrap();
        assert_eq!(status.state, TimelineProjectionStateDto::Complete);
        assert_eq!(
            state(&fixture, CapabilityKindDto::Timeline),
            CapabilityStateDto::Ready
        );
        assert_eq!(
            state(&fixture, CapabilityKindDto::Graph),
            CapabilityStateDto::Ready
        );
        assert_eq!(
            runtime.tasks.task_count_for_case(&fixture.case.meta.id.0),
            0
        );
    }
    assert_eq!(
        state(&fixture, CapabilityKindDto::Artifacts),
        CapabilityStateDto::Queued
    );
}

#[test]
fn rejected_registration_cannot_leave_a_retry_queued() {
    let fixture = import_directory();
    let runtime = runtime();
    post_import_service::cancel_capability(
        &runtime,
        &fixture.case.meta.id,
        &fixture.case.case_root,
        &fixture.source.0,
        post_import_service::ExecutableCapability::Artifacts,
    )
    .unwrap();
    runtime
        .tasks
        .retire_case_and_drain(&fixture.case.meta.id.0, Duration::from_secs(1));
    assert!(post_import_service::retry_capability(
        &runtime,
        &fixture.case.meta.id,
        &fixture.case.case_root,
        &fixture.source.0,
        post_import_service::ExecutableCapability::Artifacts,
    )
    .is_err());
    assert_eq!(
        state(&fixture, CapabilityKindDto::Artifacts),
        CapabilityStateDto::Cancelled
    );
    assert_eq!(
        runtime.tasks.task_count_for_case(&fixture.case.meta.id.0),
        0
    );
}

#[test]
fn explicit_artifact_retry_executes_a_new_canonical_attempt() {
    use persistence_sqlite::repositories::processing_phase_repo::{
        DataSourceProcessingPhaseRepo, ProcessingPhase,
    };
    let fixture = import_directory();
    let runtime = runtime();
    post_import_service::enqueue_case_pending(
        &runtime,
        &fixture.case.meta.id,
        &fixture.case.case_root,
    )
    .unwrap();
    for (_, result) in runtime.tasks.wait_all(Duration::from_secs(20)) {
        result.unwrap();
    }
    let conn = fixture.case.connection().unwrap();
    let repo = DataSourceProcessingPhaseRepo::new(&conn);
    let before = repo
        .find(&fixture.source, ProcessingPhase::Artifacts)
        .unwrap()
        .unwrap();
    assert!(matches!(
        post_import_service::retry_capability(
            &runtime,
            &fixture.case.meta.id,
            &fixture.case.case_root,
            &fixture.source.0,
            post_import_service::ExecutableCapability::Artifacts,
        )
        .unwrap(),
        post_import_service::RetryOutcome::Accepted
    ));
    for (_, result) in runtime.tasks.wait_all(Duration::from_secs(20)) {
        result.unwrap();
    }
    let after = repo
        .find(&fixture.source, ProcessingPhase::Artifacts)
        .unwrap()
        .unwrap();
    assert_ne!(before.attempt_id, after.attempt_id);
    drop(conn);
    assert_eq!(
        state(&fixture, CapabilityKindDto::Artifacts),
        CapabilityStateDto::Ready
    );
}

#[test]
fn android_partial_extraction_publishes_available_facts_and_a_partial_capability() {
    let fixture = import_directory_with_entries(
        DataSourcePlatform::Android,
        &[
            ("system/build.prop", "ro.product.model=Inspection phone\n"),
            ("system/packages.xml", "<packages><package"),
        ],
    );
    let runtime = runtime();
    post_import_service::enqueue_case_pending(
        &runtime,
        &fixture.case.meta.id,
        &fixture.case.case_root,
    )
    .unwrap();
    for (_, result) in runtime.tasks.wait_all(Duration::from_secs(20)) {
        result.unwrap();
    }
    assert_eq!(
        runtime.tasks.task_count_for_case(&fixture.case.meta.id.0),
        0
    );
    assert_eq!(
        state(&fixture, CapabilityKindDto::Artifacts),
        CapabilityStateDto::Partial
    );
    let conn = fixture.case.connection().unwrap();
    let facts = app_services::analysis_service::get_source_android_device_info(
        &conn,
        &fixture.case.case_root,
        &fixture.case.meta.id,
        &fixture.source,
    )
    .unwrap();
    assert_eq!(facts.model.as_deref(), Some("Inspection phone"));
}

#[test]
fn case_retirement_revokes_and_drains_queued_capabilities() {
    let fixture = import_directory();
    let runtime = runtime();
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = Arc::clone(&cancel);
    let (entered, observed) = mpsc::channel();
    runtime
        .tasks
        .spawn_scoped_parallel(
            "source-operation".into(),
            TaskScope::data_source(
                &fixture.case.meta.id.0,
                &fixture.source.0,
                "source-operation",
            ),
            TaskPriority::Interactive,
            cancel,
            move || {
                entered.send(()).unwrap();
                while !worker_cancel.load(Ordering::Acquire) {
                    std::thread::yield_now();
                }
                Ok(())
            },
        )
        .unwrap();
    observed.recv_timeout(Duration::from_secs(2)).unwrap();
    post_import_service::enqueue_case_pending(
        &runtime,
        &fixture.case.meta.id,
        &fixture.case.case_root,
    )
    .unwrap();
    let conn = fixture.case.connection().unwrap();
    assert_eq!(
        case_service::mark_open_capabilities_cancelled(&conn, &fixture.case.meta.id.0).unwrap(),
        2
    );
    drop(conn);
    runtime
        .tasks
        .retire_case_and_drain(&fixture.case.meta.id.0, Duration::from_secs(5));
    assert_eq!(
        runtime.tasks.task_count_for_case(&fixture.case.meta.id.0),
        0
    );
    assert_eq!(
        state(&fixture, CapabilityKindDto::Timeline),
        CapabilityStateDto::Cancelled
    );
    assert_eq!(
        state(&fixture, CapabilityKindDto::Artifacts),
        CapabilityStateDto::Cancelled
    );
}

#[test]
fn graph_write_failure_keeps_the_published_generation_and_retry_completes() {
    let fixture = import_directory();
    let conn = fixture.case.connection().unwrap();
    let source = source_db::open_source_db(&fixture.case.case_root, &fixture.source).unwrap();
    let before =
        persistence_source::control::bound_source_generation_root(&source, &fixture.source)
            .unwrap();
    source.execute_batch("CREATE TRIGGER reject_timeline_graph BEFORE INSERT ON graph_nodes WHEN NEW.node_type = 'timeline_event' BEGIN SELECT RAISE(FAIL, 'graph write rejected'); END;").unwrap();
    let cancel = AtomicBool::new(false);
    assert!(rebuild_timeline_projection(
        &conn,
        &fixture.case.case_root,
        &fixture.case.meta.id,
        Some(&fixture.source.0),
        TimelineProjectionPolicy::default(),
        &cancel
    )
    .is_err());
    assert_eq!(
        persistence_source::control::bound_source_generation_root(&source, &fixture.source)
            .unwrap(),
        before
    );
    assert_eq!(
        get_timeline_projection_status(&conn, &fixture.case.case_root, &fixture.case.meta.id)
            .unwrap()
            .state,
        TimelineProjectionStateDto::Deferred
    );
    source
        .execute_batch("DROP TRIGGER reject_timeline_graph")
        .unwrap();
    let status = rebuild_timeline_projection(
        &conn,
        &fixture.case.case_root,
        &fixture.case.meta.id,
        Some(&fixture.source.0),
        TimelineProjectionPolicy::default(),
        &cancel,
    )
    .unwrap();
    assert_eq!(status.state, TimelineProjectionStateDto::Complete);
    assert!(status
        .sources
        .iter()
        .any(|row| row.data_source_id == fixture.source.0 && row.inserted_events > 0));
}
