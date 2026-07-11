use app_services::{
    artifact_service, correlation, graph_service, report::ReportError, search_service, source_db,
    timeline_service, v3_governance_service,
};
use domain::{
    Artifact, ArtifactId, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance,
    TimelineEvent, TimelineEventId,
};
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    timeline_repo::TimelineRepo,
};
use transport::{ErrorCategory, ServiceErrorCategory};

#[test]
fn case_aggregators_ignore_sources_until_import_is_ready() {
    let temp = tempfile::TempDir::new().expect("create case parent");
    let active = app_services::case_service::create_case(
        temp.path(),
        "ready-source-aggregation",
        Some("stage2-review"),
    )
    .expect("create case");

    active
        .with_conn(|case_conn| {
            register_source(case_conn, &active, "ready", "windows", "ready", true)?;
            register_source(case_conn, &active, "pending", "windows", "pending", false)?;
            register_source(
                case_conn,
                &active,
                "importing",
                "windows",
                "importing",
                false,
            )?;
            register_source(case_conn, &active, "failed", "windows", "failed", false)?;

            let connections = source_db::open_ready_source_connections(
                case_conn,
                &active.case_root,
                &active.meta.id,
            )
            .map_err(service_error)?;
            assert_eq!(connections.len(), 1);
            assert_eq!(connections[0].0, DataSourceId("ready".to_string()));
            drop(connections);

            let artifacts = artifact_service::get_artifact_rows_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
                None,
            )
            .map_err(service_error)?;
            assert!(artifacts.is_empty());

            let timeline = timeline_service::query_timeline_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
                0,
                10,
            )
            .map_err(service_error)?;
            assert_eq!(timeline.total, 0);

            let graph = graph_service::get_graph_snapshot_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id.0,
            )
            .map_err(service_error)?;
            assert_eq!(graph.total_nodes, 0);

            let correlation = correlation::get_correlation_snapshot_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
            )
            .map_err(service_error)?;
            assert_eq!(correlation.lead_count, 0);

            let metrics = app_services::case_service::get_case_metrics_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
            )
            .map_err(service_error)?;
            assert_eq!(metrics.data_source_count, 4);
            assert_eq!(metrics.indexed_file_count, 0);

            let roots = app_services::file_service::get_file_tree_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
                false,
            )
            .map_err(service_error)?;
            assert!(roots.is_empty());

            let recent = app_services::file_service::get_recent_objects_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
            )
            .map_err(service_error)?;
            assert!(recent.is_empty());

            let search = app_services::search_service::search_files_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
                "fixture",
                0,
                10,
            )
            .map_err(service_error)?;
            assert_eq!(search.total, 0);
            Ok(())
        })
        .expect("aggregate only ready sources");
}

#[test]
fn ready_source_router_rejects_unsupported_platform_before_database_open() {
    let temp = tempfile::TempDir::new().expect("create case parent");
    let active = app_services::case_service::create_case(
        temp.path(),
        "unsupported-ready-source",
        Some("stage2-review"),
    )
    .expect("create case");

    active
        .with_conn(|case_conn| {
            register_source(case_conn, &active, "retired", "macos", "ready", false)?;
            let error = source_db::open_ready_source_connections(
                case_conn,
                &active.case_root,
                &active.meta.id,
            )
            .expect_err("unsupported ready platform must fail before opening its source DB");
            let message = error.to_string();
            assert!(message.contains("platform is unsupported"));
            assert!(!message.contains("source DB is missing"));
            Ok(())
        })
        .expect("reject unsupported platform");
}

#[test]
fn scoped_reads_reject_importing_source_even_when_source_database_contains_rows() {
    let temp = tempfile::TempDir::new().expect("create case parent");
    let active = app_services::case_service::create_case(
        temp.path(),
        "scoped-readiness",
        Some("stage2-review"),
    )
    .expect("create case");

    active
        .with_conn(|case_conn| {
            register_source(case_conn, &active, "importing", "linux", "importing", true)?;
            let source_id = DataSourceId("importing".to_string());
            let source_conn = source_db::open_registered_source_db(
                case_conn,
                &active.case_root,
                &source_id,
            )?;
            source_conn.execute(
                "INSERT INTO file_entries
                 (id, parent_id, data_source_id, path, name, entry_type, size, deleted, hidden, system)
                 VALUES ('file-1', NULL, 'importing', '/fixture.txt', 'fixture.txt', 'file', 7, 0, 0, 0)",
                [],
            )?;
            ArtifactRepo::new(&source_conn).insert_batch(
                &[Artifact {
                    id: ArtifactId("artifact-1".to_string()),
                    family: "LinuxShellHistory".to_string(),
                    title: "fixture".to_string(),
                    summary: "fixture".to_string(),
                    source_object_id: Some(domain::FileEntryId("file-1".to_string())),
                    extractor_id: Some("fixture".to_string()),
                    extractor_version: Some("1".to_string()),
                    confidence: Some(1.0),
                    source_attribution: Some("fixture".to_string()),
                    created_at: chrono::Utc::now(),
                    attrs: Default::default(),
                }],
                &active.meta.id.0,
                &source_id.0,
            )?;
            TimelineRepo::new(&source_conn).insert_batch_with_case(
                &[TimelineEvent {
                    id: TimelineEventId("event-1".to_string()),
                    source_object_id: "file-1".to_string(),
                    event_type: "LinuxShellHistory".to_string(),
                    timestamp: chrono::Utc::now(),
                    title: "fixture".to_string(),
                    description: "fixture".to_string(),
                    parser_id: Some("fixture".to_string()),
                    parser_version: Some("1".to_string()),
                    confidence: Some(1.0),
                    source_attribution: Some("fixture".to_string()),
                    attrs: Default::default(),
                }],
                &active.meta.id.0,
            )?;

            let file_id = "ds:importing:file-1";
            let file_error = app_services::file_service::open_file_handle_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
                file_id,
            )
            .expect_err("file preview must reject importing source");
            let artifact_error = artifact_service::get_artifact_row_by_id_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
                "ds:importing:artifact-1",
            )
            .expect_err("artifact detail must reject importing source");
            let timeline_error = timeline_service::get_timeline_event_by_id_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id,
                "ds:importing:event-1",
            )
            .expect_err("timeline detail must reject importing source");

            for message in [
                file_error.to_string(),
                artifact_error.to_string(),
                timeline_error.to_string(),
            ] {
                assert!(message.contains("not ready"), "{message}");
            }
            assert!(matches!(
                file_error.category(),
                ErrorCategory::Validation
            ));
            assert!(matches!(
                artifact_error.category(),
                ErrorCategory::Validation
            ));
            assert!(matches!(
                timeline_error.category(),
                ErrorCategory::Validation
            ));
            Ok(())
        })
        .expect("validate scoped ready-source boundary");
}

#[test]
fn ready_source_errors_keep_public_service_categories() {
    let temp = tempfile::TempDir::new().expect("create case parent");
    let active = app_services::case_service::create_case(
        temp.path(),
        "ready-source-error-categories",
        Some("stage2-review"),
    )
    .expect("create case");

    active
        .with_conn(|case_conn| {
            register_source(case_conn, &active, "unsupported", "macos", "ready", true)?;
            let source_id = DataSourceId("unsupported".to_string());
            let ready_error = unsupported_ready_source_error(case_conn, &active, &source_id);

            let graph_error = graph_service::GraphServiceError::from(ready_error);
            assert!(matches!(graph_error.category(), ErrorCategory::Unsupported));

            let ready_error = unsupported_ready_source_error(case_conn, &active, &source_id);
            let file_error = app_services::file_service::FileServiceError::from(ready_error);
            assert!(matches!(file_error.category(), ErrorCategory::Unsupported));

            let ready_error = unsupported_ready_source_error(case_conn, &active, &source_id);
            let search_error = search_service::SearchError::from(ready_error);
            assert!(matches!(
                search_error.category(),
                ErrorCategory::Unsupported
            ));

            let governance_error = v3_governance_service::V3GovernanceError::from(graph_error);
            assert!(matches!(
                governance_error.category(),
                ErrorCategory::Unsupported
            ));

            let governance_error = v3_governance_service::get_v3_governance_snapshot_for_case(
                case_conn,
                &active.case_root,
                &active.meta.id.0,
            )
            .expect_err("governance must reject unsupported ready sources");
            assert!(matches!(
                governance_error.category(),
                ErrorCategory::Unsupported
            ));

            let report_error = ReportError::from(unsupported_ready_source_error(
                case_conn, &active, &source_id,
            ));
            assert!(matches!(
                report_error.category(),
                ErrorCategory::Unsupported
            ));

            let importing_error = source_db::ReadySourceError::NotReady {
                data_source_id: "importing".to_string(),
                state: "importing".to_string(),
            };
            let report_error = ReportError::from(importing_error);
            assert!(matches!(report_error.category(), ErrorCategory::Validation));
            Ok(())
        })
        .expect("preserve ready-source error categories");
}

fn unsupported_ready_source_error(
    case_conn: &rusqlite::Connection,
    active: &app_services::active_case::ActiveCase,
    source_id: &DataSourceId,
) -> source_db::ReadySourceError {
    match source_db::open_ready_source_by_id(
        case_conn,
        &active.case_root,
        &active.meta.id,
        source_id,
    ) {
        Ok(_) => panic!("unsupported source must fail closed"),
        Err(error) => error,
    }
}

fn register_source(
    case_conn: &rusqlite::Connection,
    active: &app_services::active_case::ActiveCase,
    id: &str,
    platform: &str,
    import_state: &str,
    create_database: bool,
) -> persistence_sqlite::DbResult<()> {
    let source = DataSource {
        id: DataSourceId(id.to_string()),
        name: id.to_string(),
        kind: DataSourceKind::E01,
        source_path: active.case_root.join(format!("{id}.E01")),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(id, Some(platform), None);
    storage.import_state = import_state.to_string();
    DataSourceRepo::new(case_conn).insert_with_storage(&active.meta.id, &source, &storage)?;

    if create_database {
        let source_conn = source_db::open_source_db(&active.case_root, &source.id)?;
        DataSourceRepo::new(&source_conn).upsert_source_local_metadata(&active.meta.id, &source)?;
    }
    Ok(())
}

fn service_error(error: impl std::fmt::Display) -> persistence_sqlite::DbError {
    persistence_sqlite::DbError::System(error.to_string())
}
