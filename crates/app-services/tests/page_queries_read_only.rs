//! Query entry points must preserve source files even when no WAL exists yet.
use app_services::{
    analysis_service as analysis, case_projection_service, case_service, correlation,
    graph_service, infrastructure_graph_service, source_db, timeline_service,
    v2_governance_service, v3_governance_service,
};
use domain::{DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_sqlite::repositories::{
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    graph_repo::GraphRepo,
    timeline_repo::TimelineRepo,
};
use rusqlite::Connection;
use std::path::Path;
use std::sync::atomic::AtomicBool;

fn source(
    case: &Connection,
    root: &Path,
    case_id: &domain::CaseId,
    id: &str,
    platform: &str,
) -> DataSourceId {
    let evidence = root.join(format!("evidence-{id}"));
    std::fs::create_dir_all(&evidence).unwrap();
    std::fs::write(evidence.join("one.txt"), b"bounded query bytes").unwrap();
    let source = DataSource {
        id: DataSourceId(id.into()),
        name: id.into(),
        kind: DataSourceKind::LogicalDirectory,
        source_path: evidence,
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(id, Some(platform), None);
    storage.import_state = "ready".into();
    DataSourceRepo::new(case)
        .insert_with_storage(case_id, &source, &storage)
        .unwrap();
    let db = source_db::open_source_db(root, &source.id).unwrap();
    DataSourceRepo::new(&db)
        .upsert_source_local_metadata(case_id, &source)
        .unwrap();
    db.execute("INSERT INTO file_entries(id,data_source_id,path,name,entry_type,size) VALUES('file',?1,'one.txt','one.txt','file',19)", [&id]).unwrap();
    let event = domain::TimelineEvent {
        id: domain::TimelineEventId("event".into()),
        source_object_id: "file".into(),
        event_type: "FILE_MODIFIED".into(),
        timestamp: chrono::Utc::now(),
        title: "fixture".into(),
        description: String::new(),
        parser_id: Some("fixture".into()),
        parser_version: Some("1".into()),
        confidence: Some(1.0),
        source_attribution: None,
        attrs: Default::default(),
    };
    TimelineRepo::new(&db)
        .insert_batch_with_case(&[event], &case_id.0)
        .unwrap();
    db.pragma_update(None, "journal_mode", "DELETE").unwrap();
    source.id
}

fn assert_source_unchanged(root: &Path, id: &DataSourceId, read: impl FnOnce()) {
    let path = source_db::source_db_path(root, id);
    let observer = persistence_sqlite::open_existing_source_read_only(&path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let version: i64 = observer
        .pragma_query_value(None, "data_version", |row| row.get(0))
        .unwrap();
    read();
    assert_eq!(
        observer
            .pragma_query_value(None, "data_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        version
    );
    assert_eq!(
        observer
            .pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))
            .unwrap(),
        "delete"
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(!std::path::PathBuf::from(format!("{}-wal", path.display())).exists());
    assert!(!std::path::PathBuf::from(format!("{}-shm", path.display())).exists());
}

#[test]
fn windows_analysis_queries_preserve_source_database_and_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let active = case_service::create_case(temp.path(), "read", None).unwrap();
    let id = active
        .with_conn(|db| {
            Ok(source(
                db,
                &active.case_root,
                &active.meta.id,
                "windows",
                "windows",
            ))
        })
        .unwrap();
    let case = persistence_sqlite::open_existing_source_read_only(&active.db_path()).unwrap();
    let runtime = analysis::AnalysisSourceReadRuntime::default();
    assert_source_unchanged(&active.case_root, &id, || {
        let (root, key) = (&active.case_root, &active.meta.id);
        assert!(!analysis::classify_source_files(&case, root, key, &id, 10)
            .unwrap()
            .is_empty());
        analysis::get_file_classification_board(&case, root, key, &id, 10, &runtime).unwrap();
        analysis::get_source_evidence_summary(&case, root, key, &id).unwrap();
        analysis::get_source_system_info(&case, root, key, &id, &runtime).unwrap();
        analysis::generate_source_analysis_summary(&case, root, key, &id, &runtime).unwrap();
        analysis::get_source_registry_summary(&case, root, key, &id, 0, 10).unwrap();
        analysis::get_source_registry_structured_summary(&case, root, key, &id).unwrap();
        analysis::get_source_browser_summary(&case, root, key, &id, 0, 10).unwrap();
        analysis::get_source_email_summary(&case, root, key, &id, 0, 10).unwrap();
        analysis::get_source_evtx_summary(&case, root, key, &id, None, 0, 10).unwrap();
    });
    assert_eq!(
        std::fs::read(active.case_root.join("evidence-windows/one.txt")).unwrap(),
        b"bounded query bytes"
    );
}

#[test]
fn linux_and_android_summary_queries_do_not_open_writers() {
    let temp = tempfile::tempdir().unwrap();
    let active = case_service::create_case(temp.path(), "read-platforms", None).unwrap();
    let ids = active
        .with_conn(|db| {
            Ok([
                source(db, &active.case_root, &active.meta.id, "linux", "linux"),
                source(db, &active.case_root, &active.meta.id, "android", "android"),
            ])
        })
        .unwrap();
    let case = persistence_sqlite::open_existing_source_read_only(&active.db_path()).unwrap();
    assert_source_unchanged(&active.case_root, &ids[0], || {
        analysis::get_source_linux_summary(
            &case,
            &active.case_root,
            &active.meta.id,
            &ids[0],
            0,
            10,
        )
        .unwrap();
    });
    assert_source_unchanged(&active.case_root, &ids[1], || {
        analysis::get_source_android_device_info(
            &case,
            &active.case_root,
            &active.meta.id,
            &ids[1],
        )
        .unwrap();
        analysis::get_source_android_package_summary(
            &case,
            &active.case_root,
            &active.meta.id,
            &ids[1],
            0,
            10,
            &analysis::AnalysisSourceReadRuntime::default(),
        )
        .unwrap();
    });
}

#[test]
fn timeline_pages_facets_and_detail_are_read_only() {
    let temp = tempfile::tempdir().unwrap();
    let active = case_service::create_case(temp.path(), "timeline-read", None).unwrap();
    let id = active
        .with_conn(|db| {
            Ok(source(
                db,
                &active.case_root,
                &active.meta.id,
                "timeline",
                "windows",
            ))
        })
        .unwrap();
    let case = persistence_sqlite::open_existing_source_read_only(&active.db_path()).unwrap();
    assert_source_unchanged(&active.case_root, &id, || {
        let page = timeline_service::query_timeline_filtered_for_case(
            &case,
            &active.case_root,
            &active.meta.id,
            timeline_service::TimelineQuery::unfiltered(0, 10),
        )
        .unwrap();
        assert_eq!(page.total, 1);
        let detail = timeline_service::get_timeline_event_by_id_for_case(
            &case,
            &active.case_root,
            &active.meta.id,
            &page.items[0].id,
        )
        .unwrap()
        .unwrap();
        assert_eq!(detail.id, page.items[0].id);
        let facets = timeline_service::get_timeline_facets_for_case(
            &case,
            &active.case_root,
            &active.meta.id,
            None,
            None,
            None,
            10,
        )
        .unwrap();
        assert_eq!(facets.total_events, 1);
    });
}

#[test]
fn dashboard_graph_correlation_and_infrastructure_queries_do_not_build_or_replace() {
    let temp = tempfile::tempdir().unwrap();
    let active = case_service::create_case(temp.path(), "dashboard-read", None).unwrap();
    let id = active
        .with_conn(|db| {
            Ok(source(
                db,
                &active.case_root,
                &active.meta.id,
                "dashboard",
                "linux",
            ))
        })
        .unwrap();
    let case_path = active.db_path();
    let case_before = std::fs::read(&case_path).unwrap();
    let graph_path = active.case_root.join("indexes/case-graph.db");
    assert!(!graph_path.exists());
    let case = persistence_sqlite::open_existing_source_read_only(&case_path).unwrap();

    assert_source_unchanged(&active.case_root, &id, || {
        correlation::get_correlation_snapshot_read_only_for_case(
            &case,
            &active.case_root,
            &active.meta.id,
        )
        .unwrap();
        graph_service::get_graph_snapshot_for_case_read_only(
            &case,
            &active.case_root,
            &active.meta.id.0,
        )
        .unwrap();
        graph_service::query_graph_for_case_read_only(
            &case,
            &active.case_root,
            &active.meta.id.0,
            transport::dto::GraphQueryDto {
                start_ids: Vec::new(),
                edge_types: Vec::new(),
                max_depth: 1,
                confidence_floor: None,
                limit: 10,
                edge_limit: 10,
            },
        )
        .unwrap();
        graph_service::get_node_neighborhood_for_case_read_only(
            &case,
            &active.case_root,
            &active.meta.id.0,
            "ds:dashboard:file",
            1,
        )
        .unwrap();
        graph_service::get_provenance_chain_for_case_read_only(
            &case,
            &active.case_root,
            &active.meta.id.0,
            "ds:dashboard:edge-missing",
        )
        .unwrap_err();
        v2_governance_service::get_v2_governance_snapshot_for_case_read_only(
            &case,
            &active.case_root,
            &active.meta.id.0,
        )
        .unwrap();
        v3_governance_service::get_v3_governance_snapshot_for_case_read_only(
            &case,
            &active.case_root,
            &active.meta.id.0,
        )
        .unwrap();
        v3_governance_service::get_case_overview_snapshot_for_case_read_only(
            &case,
            &active.case_root,
            &active.meta.id.0,
        )
        .unwrap();
        infrastructure_graph_service::get_infrastructure_graph_read_only(
            &case,
            &active.case_root,
            &active.meta.id,
        )
        .unwrap();
    });
    assert_eq!(std::fs::read(&case_path).unwrap(), case_before);
    assert!(!graph_path.exists());
}

#[test]
fn projection_builder_publishes_ready_then_reports_stale_generation() {
    let temp = tempfile::tempdir().unwrap();
    let active = case_service::create_case(temp.path(), "projection-builder", None).unwrap();
    let id = active
        .with_conn(|db| {
            Ok(source(
                db,
                &active.case_root,
                &active.meta.id,
                "projection",
                "linux",
            ))
        })
        .unwrap();
    let case_path = active.db_path();
    let write_case = app_services::connection::open_existing_case_db(&case_path).unwrap();
    let report = case_projection_service::rebuild_case_projections(
        &write_case,
        &active.case_root,
        &active.meta.id.0,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(report.graph_rebuilt && report.correlation_rebuilt && report.infrastructure_rebuilt);
    assert!(active.case_root.join("indexes/case-graph.db").is_file());

    let read_case = persistence_sqlite::open_existing_source_read_only(&case_path).unwrap();
    let ready = graph_service::get_graph_snapshot_for_case_read_only(
        &read_case,
        &active.case_root,
        &active.meta.id.0,
    )
    .unwrap();
    assert_eq!(ready.projection_status, "ready");

    let source_conn = source_db::open_source_db(&active.case_root, &id).unwrap();
    GraphRepo::new(&source_conn)
        .insert_nodes_batch(&[domain::GraphNode {
            id: "projection-new-node".to_string(),
            case_id: active.meta.id.0.clone(),
            node_type: domain::NodeType::Artifact,
            label: "new artifact".to_string(),
            summary: "forces a new source manifest".to_string(),
            tags: vec!["fixture".to_string()],
            created_at: "2026-10-23T00:00:00Z".to_string(),
        }])
        .unwrap();

    let stale = graph_service::get_graph_snapshot_for_case_read_only(
        &read_case,
        &active.case_root,
        &active.meta.id.0,
    )
    .unwrap();
    assert_eq!(stale.projection_status, "stale");
}

#[test]
fn projection_builder_honors_pre_cancelled_work() {
    let temp = tempfile::tempdir().unwrap();
    let active = case_service::create_case(temp.path(), "projection-cancel", None).unwrap();
    let case_path = active.db_path();
    let connection = app_services::connection::open_existing_case_db(&case_path).unwrap();
    let error = case_projection_service::rebuild_case_projections(
        &connection,
        &active.case_root,
        &active.meta.id.0,
        &AtomicBool::new(true),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        case_projection_service::CaseProjectionError::Cancelled
    ));
    assert!(!active.case_root.join("indexes/case-graph.db").exists());
}
