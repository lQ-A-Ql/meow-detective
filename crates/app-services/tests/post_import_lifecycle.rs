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
    mpsc, Arc,
};
use std::time::Duration;
use transport::dto::{CapabilityKindDto, CapabilityStateDto, TimelineProjectionStateDto};

struct ImportedCase {
    _root: tempfile::TempDir,
    case: app_services::active_case::ActiveCase,
    source: DataSourceId,
}

fn import_directory() -> ImportedCase {
    let root = tempfile::tempdir().unwrap();
    let evidence = root.path().join("evidence");
    std::fs::create_dir_all(&evidence).unwrap();
    std::fs::write(evidence.join("document.txt"), "investigation text\n").unwrap();
    let case = case_service::create_case(root.path(), "capability lifecycle", None).unwrap();
    let source = case
        .with_conn(|conn| {
            let config = app_services::import_precheck::prepare_import_source_config_from_path(
                &evidence.to_string_lossy(),
                DataSourcePlatform::Windows,
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
    let runtime = runtime();
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
