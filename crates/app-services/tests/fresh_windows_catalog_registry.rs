use app_services::{
    analysis_service,
    bitlocker_runtime::BitLockerUnlockRegistry,
    file_service::{self, PreviewRuntimeRegistry},
    import_analysis::ImportAnalysisMode,
    import_job_coordinator::{run_single_import, SingleImportOutcome, SingleImportRequest},
    import_precheck,
    registry_browser_service::{self, RegistryBrowserContext, RegistryBrowserRegistry},
    source_db,
};
use persistence_source::{
    fjall::instrumentation::ProcessMeasurement, repositories::file_repo::FileRepo,
};
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
    time::{Instant, UNIX_EPOCH},
};
use transport::{commands::GetFileRowsRequest, dto::RegistryHiveListRequestDto};

#[test]
#[ignore = "requires FORENSICS_FRESH_WINDOWS_E01 and FORENSICS_FRESH_WINDOWS_OUTPUT"]
fn fresh_windows_catalog_and_registry_pages() {
    run_fresh(true);
}

#[test]
#[ignore = "requires FORENSICS_FRESH_WINDOWS_E01 and FORENSICS_FRESH_WINDOWS_OUTPUT"]
fn fresh_windows_directory_pages() {
    run_fresh(false);
}

fn run_fresh(build_timeline: bool) {
    let image = PathBuf::from(
        std::env::var_os("FORENSICS_FRESH_WINDOWS_E01").expect("set FORENSICS_FRESH_WINDOWS_E01"),
    );
    let output = PathBuf::from(
        std::env::var_os("FORENSICS_FRESH_WINDOWS_OUTPUT")
            .expect("set FORENSICS_FRESH_WINDOWS_OUTPUT"),
    );
    std::fs::create_dir_all(&output).unwrap();
    let (logs, _log_guard) = infrastructure::logging::LogRuntime::start(
        infrastructure::logging::LogConfig::new(output.join("logs")),
    )
    .unwrap();
    use tracing_subscriber::prelude::*;
    tracing_subscriber::registry().with(logs.layer()).init();
    tracing::info!("Fresh Windows catalog and registry validation started");
    let input_before = input_metadata(&image);
    std::fs::write(
        output.join("input-before.json"),
        serde_json::to_vec_pretty(&input_before).unwrap(),
    )
    .unwrap();
    let active =
        app_services::case_service::create_case(&output.join("cases"), "fresh-windows-pages", None)
            .unwrap();
    std::fs::write(
        output.join("case-location.json"),
        serde_json::to_vec_pretty(&json!({
            "caseRoot": active.case_root,
            "caseId": active.meta.id.0,
            "image": image,
        }))
        .unwrap(),
    )
    .unwrap();
    let case_root = active.case_root.clone();
    let started = Instant::now();
    let source_id = active
        .with_conn(|connection| {
            let job = JobRepo::new(connection).create(&active.meta.id.0, "Fresh Windows import")?;
            let config = import_precheck::prepare_import_source_config_from_path(
                image.to_str().unwrap(),
                domain::DataSourcePlatform::Windows,
            )
            .unwrap();
            eprintln!("FRESH_WINDOWS_STAGE catalog-import");
            let outcome = run_single_import(SingleImportRequest {
                connection,
                case_root: &active.case_root,
                case_id: &active.meta.id,
                job_id: &job,
                import_config: config,
                max_import_workers: Some(2),
                max_analysis_workers: Some(2),
                analysis_mode: ImportAnalysisMode::MetadataOnly,
                cancel_token: &Arc::new(AtomicBool::new(false)),
                event_sink: None,
            })
            .unwrap();
            assert!(matches!(outcome, SingleImportOutcome::Completed(_)));
            let source_id = DataSourceRepo::new(connection)
                .find_by_case(&active.meta.id)?
                .first()
                .unwrap()
                .id
                .clone();
            let import_elapsed_ms = started.elapsed().as_millis();
            eprintln!("FRESH_WINDOWS_STAGE registry-extraction");
            let extraction_started = Instant::now();
            let extraction = analysis_service::run_source_analysis_extraction(
                connection,
                &active.case_root,
                &active.meta.id,
                &source_id,
                &["Registry"],
            )
            .unwrap();
            std::fs::write(
                output.join("extraction.json"),
                serde_json::to_vec_pretty(&json!({
                    "sourceId": source_id.0,
                    "catalogImportElapsedMs": import_elapsed_ms,
                    "registryElapsedMs": extraction_started.elapsed().as_millis(),
                    "extraction": extraction,
                }))
                .unwrap(),
            )?;
            Ok(source_id)
        })
        .unwrap();
    if build_timeline {
        eprintln!("FRESH_WINDOWS_STAGE managed-timeline");
        let runtime = app_services::post_import_service::PostImportRuntime {
            tasks: Arc::new(app_services::task_runtime::TaskManager::new()),
            bitlocker: Arc::new(BitLockerUnlockRegistry::default()),
            events: None,
        };
        let timeline_started = Instant::now();
        let timeline = app_services::post_import_service::rebuild_timeline(
            &runtime,
            &active.meta.id,
            &active.case_root,
            Some(&source_id.0),
        )
        .unwrap();
        std::fs::write(
            output.join("timeline.json"),
            serde_json::to_vec_pretty(&json!({
                "elapsedMs": timeline_started.elapsed().as_millis(), "status": timeline,
            }))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            timeline.state,
            transport::dto::TimelineProjectionStateDto::Complete
        );
    }
    drop(active);
    let input_after = input_metadata(&image);
    assert_eq!(
        input_before, input_after,
        "evidence segment metadata changed"
    );
    std::fs::write(
        output.join("input-after.json"),
        serde_json::to_vec_pretty(&input_after).unwrap(),
    )
    .unwrap();
    verify_pages(&case_root, &output.join("pages.json"));
    logs.flush().unwrap();
    let snapshot = app_services::application_log_service::ApplicationLogService::new(logs)
        .read(transport::dto::ApplicationLogRequestDto {
            direction: transport::dto::ApplicationLogDirectionDto::Latest,
            cursors: Vec::new(),
            filter: Default::default(),
            limit_per_level: 100,
        })
        .unwrap();
    assert!(snapshot.pages.iter().any(|page| page.emitted > 0));
    assert!(snapshot.writer.failure.is_none());
    std::fs::write(
        output.join("application-logs.json"),
        serde_json::to_vec_pretty(&snapshot).unwrap(),
    )
    .unwrap();
}

fn input_metadata(image: &Path) -> Vec<(PathBuf, u64, u128, bool)> {
    image_e01::E01Reader::open(image)
        .unwrap()
        .open_plan()
        .unwrap()
        .segment_paths()
        .into_iter()
        .map(|path| {
            let metadata = std::fs::metadata(&path).unwrap();
            let modified = metadata
                .modified()
                .unwrap()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            (
                path,
                metadata.len(),
                modified,
                metadata.permissions().readonly(),
            )
        })
        .collect()
}

fn verify_pages(case_root: &Path, report_path: &Path) {
    let meta: domain::CaseMeta =
        serde_json::from_slice(&std::fs::read(case_root.join("case.json")).unwrap()).unwrap();
    let connection = rusqlite::Connection::open_with_flags(
        case_root.join("app.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let sources = DataSourceRepo::new(&connection)
        .find_by_case(&meta.id)
        .unwrap();
    assert_eq!(sources.len(), 1);
    let source_id = &sources[0].id;
    let source =
        source_db::open_registered_source_db_read_only(&connection, case_root, source_id).unwrap();
    eprintln!("FRESH_WINDOWS_STAGE file-tree");
    let roots =
        file_service::get_file_tree_for_case(&connection, case_root, &meta.id, true).unwrap();
    let root = roots
        .iter()
        .find(|root| root.data_source_id.as_deref() == Some(source_id.0.as_str()))
        .unwrap();
    let started = Instant::now();
    let measurement = ProcessMeasurement::start().unwrap();
    let children = file_service::get_file_children_for_case(
        &connection,
        case_root,
        &meta.id,
        &root.id,
        0,
        500,
        true,
    )
    .unwrap();
    let tree_elapsed_ms = started.elapsed().as_millis();
    let tree_reads = measurement.snapshot();
    drop(measurement);
    eprintln!("FRESH_WINDOWS_STAGE root-rows");
    let request = GetFileRowsRequest {
        parent_id: Some(root.id.clone()),
        limit: 500,
        show_hidden: true,
        ..Default::default()
    };
    let measurement = ProcessMeasurement::start().unwrap();
    let rows =
        file_service::get_file_rows_for_case(&connection, case_root, &meta.id, &request).unwrap();
    let row_reads = measurement.snapshot();
    drop(measurement);
    let visible_children = file_service::get_file_children_for_case(
        &connection,
        case_root,
        &meta.id,
        &root.id,
        0,
        500,
        false,
    )
    .unwrap();
    eprintln!("FRESH_WINDOWS_STAGE orphan-children");
    let deleted_group = children
        .children
        .iter()
        .find(|node| node.name == "$DeletedOrphans");
    let measurement = ProcessMeasurement::start().unwrap();
    let orphan_children = deleted_group.map(|group| {
        file_service::get_file_children_for_case(
            &connection,
            case_root,
            &meta.id,
            &group.id,
            0,
            25,
            true,
        )
        .unwrap()
    });
    let orphan_reads = measurement.snapshot();
    drop(measurement);
    eprintln!("FRESH_WINDOWS_STAGE hardware-summary");
    let started = Instant::now();
    let summary = analysis_service::get_source_registry_structured_summary(
        &connection,
        case_root,
        &meta.id,
        source_id,
    )
    .unwrap();
    let hardware_elapsed_ms = started.elapsed().as_millis();
    let bitlocker = Arc::new(BitLockerUnlockRegistry::default());
    let preview = PreviewRuntimeRegistry::default();
    let context = RegistryBrowserContext {
        case_conn: &connection,
        case_root,
        case_id: &meta.id,
        bitlocker: &bitlocker,
        preview: &preview,
    };
    let registry = RegistryBrowserRegistry::default();
    let measurement = ProcessMeasurement::start().unwrap();
    let started = Instant::now();
    let hives = registry_browser_service::list_hives(
        &registry,
        &context,
        RegistryHiveListRequestDto {
            data_source_id: source_id.0.clone(),
            limit: 100,
            cursor: None,
        },
    )
    .unwrap();
    let hive_elapsed_ms = started.elapsed().as_millis();
    let hive_reads = measurement.snapshot();
    drop(measurement);
    let raw_hidden = FileRepo::new(&source)
        .find_by_id(&domain::FileEntryId("mft:1:626812".into()))
        .unwrap();
    let report = json!({
        "caseRoot": case_root, "sourceId": source_id.0,
        "roots": roots, "children": children, "visibleChildren": visible_children,
        "rows": rows, "orphanChildren": orphan_children, "registry": summary,
        "hives": hives, "referenceRecord": raw_hidden,
        "catalogQueryReads": {
            "tree": {"pointReads": tree_reads.point_reads, "scanEntries": tree_reads.scan_entries},
            "rows": {"pointReads": row_reads.point_reads, "scanEntries": row_reads.scan_entries},
            "orphanChildren": {"pointReads": orphan_reads.point_reads, "scanEntries": orphan_reads.scan_entries}
        },
        "timingsMs": {"fileTree": tree_elapsed_ms, "registrySummary": hardware_elapsed_ms,
            "hiveDiscovery": hive_elapsed_ms},
        "hiveDiscoveryReads": {"pointReads": hive_reads.point_reads,
            "scanEntries": hive_reads.scan_entries, "bytesRead": hive_reads.bytes_read}
    });
    std::fs::write(report_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    assert!(
        children.total_count < 500,
        "partition root still contains excessive directories"
    );
    assert_eq!(root.expanded, Some(true));
    assert!(children
        .children
        .iter()
        .all(|node| node.expanded == Some(false)));
    let child_ids = children
        .children
        .iter()
        .map(|node| &node.id)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(child_ids.len(), children.children.len());
    assert!(rows.rows.iter().all(|row| {
        !row.path.starts_with("/$DeletedOrphans/")
            && !row.path.starts_with("/$Unresolved/")
            && !row.path.starts_with("/Unresolved/")
    }));
    assert!(deleted_group.is_some(), "deleted orphan group is missing");
    assert!(
        deleted_group.unwrap().has_children,
        "deleted orphan group cannot be expanded in the file tree"
    );
    assert!(orphan_children.unwrap().total_count > 0);
    assert!(
        orphan_reads.scan_entries <= 100,
        "orphan page scanned the full directory"
    );
    assert!(
        orphan_reads.point_reads <= 200,
        "orphan page loaded excessive catalog records"
    );
    assert!(visible_children
        .children
        .iter()
        .all(|node| !node.hidden && !node.system));
    assert!(
        !summary.hardware_devices.is_empty(),
        "published hardware summary is empty"
    );
    assert!(!hives.hives.is_empty(), "published Hive list is empty");
    assert!(
        hive_reads.point_reads < 500,
        "Hive discovery scanned the file catalog"
    );
    assert!(raw_hidden.unwrap().hidden);
}
