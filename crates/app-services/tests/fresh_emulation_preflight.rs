use app_services::{
    case_service,
    import_analysis::ImportAnalysisMode,
    import_job_coordinator::{run_single_import, SingleImportRequest},
    import_precheck, mount_service,
};
use domain::DataSourcePlatform;
use persistence_source::fjall::instrumentation::ProcessMeasurement;
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};
use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc, Barrier},
    time::Instant,
};

#[test]
#[ignore = "requires FORENSICS_PREFLIGHT_E01, FORENSICS_PREFLIGHT_PLATFORM and FORENSICS_PREFLIGHT_OUTPUT"]
fn fresh_import_reuses_preflight_across_sequential_and_concurrent_consumers() {
    let image = PathBuf::from(std::env::var_os("FORENSICS_PREFLIGHT_E01").unwrap());
    let output = PathBuf::from(std::env::var_os("FORENSICS_PREFLIGHT_OUTPUT").unwrap());
    let platform = match std::env::var("FORENSICS_PREFLIGHT_PLATFORM")
        .unwrap()
        .as_str()
    {
        "windows" => DataSourcePlatform::Windows,
        "linux" => DataSourcePlatform::Linux,
        _ => panic!("supported sample platform is required"),
    };
    std::fs::create_dir_all(&output).unwrap();
    use tracing_subscriber::prelude::*;
    let (logs, _log_guard) = infrastructure::logging::LogRuntime::start(
        infrastructure::logging::LogConfig::new(output.join("logs")),
    )
    .unwrap();
    tracing_subscriber::registry()
        .with(
            logs.layer()
                .with_filter(tracing_subscriber::filter::LevelFilter::INFO),
        )
        .init();
    let before = segment_metadata(&image);
    let active = case_service::create_case(&output.join("cases"), "fresh-preflight", None).unwrap();
    let started = Instant::now();
    active
        .with_conn(|conn| {
            let config = import_precheck::prepare_import_source_config_from_path(
                image.to_str().unwrap(),
                platform,
            )
            .unwrap();
            let job =
                JobRepo::new(conn).create(&active.meta.id.0, "Preflight acceptance import")?;
            run_single_import(SingleImportRequest {
                connection: conn,
                case_id: &active.meta.id,
                case_root: &active.case_root,
                job_id: &job,
                import_config: config,
                max_import_workers: Some(2),
                max_analysis_workers: Some(2),
                analysis_mode: ImportAnalysisMode::MetadataOnly,
                cancel_token: &Arc::new(AtomicBool::new(false)),
                event_sink: None,
            })
            .inspect_err(|_| {
                logs.flush().unwrap();
            })
            .unwrap();
            Ok(())
        })
        .unwrap();
    let import_ms = started.elapsed().as_millis();
    let conn = active.connection().unwrap();
    let source = DataSourceRepo::new(&conn)
        .find_by_case(&active.meta.id)
        .unwrap()
        .remove(0)
        .id;
    let mut results = Vec::new();
    for attempt in 0..2 {
        let work = ProcessMeasurement::start().unwrap();
        let started = Instant::now();
        let result =
            mount_service::emulation_preflight(&conn, &active.case_root, &active.meta.id, &source)
                .unwrap();
        assert!(!result.installs.is_empty());
        if platform == DataSourcePlatform::Linux {
            assert!(
                result
                    .installs
                    .iter()
                    .any(|install| install.os_release_pretty_name.is_some()),
                "Linux installation identity must be read from the filesystem"
            );
            mount_service::linux_guest_profile(&conn, &active.case_root, &active.meta.id, &source)
                .unwrap();
        }
        let counters = work.snapshot();
        assert!(
            counters.database_opens <= 1,
            "one preflight must retain its source store"
        );
        assert!(
            counters.scan_entries < 1000,
            "preflight must use bounded indexes"
        );
        if attempt == 1 {
            assert_eq!(counters.scan_calls, 0);
        }
        results.push(serde_json::json!({"attempt": attempt, "elapsedMs": started.elapsed().as_millis(), "scanEntries": counters.scan_entries, "scanCalls": counters.scan_calls, "databaseOpens": counters.database_opens, "result": result}));
    }
    mount_service::clear_emulation_preflight_cache();
    let work = ProcessMeasurement::start().unwrap();
    let barrier = Barrier::new(4);
    let responses = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..4)
            .map(|_| {
                let barrier = &barrier;
                let active = &active;
                let source = &source;
                scope.spawn(move || {
                    let conn = app_services::connection::open_existing_case_read_only(
                        &active.case_root.join("app.db"),
                    )
                    .unwrap();
                    barrier.wait();
                    mount_service::emulation_preflight(
                        &conn,
                        &active.case_root,
                        &active.meta.id,
                        source,
                    )
                    .unwrap()
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| serde_json::to_value(worker.join().unwrap()).unwrap())
            .collect::<Vec<_>>()
    });
    assert!(responses.windows(2).all(|pair| pair[0] == pair[1]));
    let concurrent = work.snapshot();
    let first_scans = results[0]["scanCalls"].as_u64().unwrap();
    assert_eq!(
        concurrent.scan_calls, first_scans,
        "four consumers must share one calculation"
    );
    drop(work);
    assert_eq!(before, segment_metadata(&image));
    let report = serde_json::json!({"importMs": import_ms, "sequential": results, "concurrentScanCalls": concurrent.scan_calls, "concurrentScanEntries": concurrent.scan_entries});
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    logs.flush().unwrap();
}

fn segment_metadata(image: &std::path::Path) -> Vec<(PathBuf, u64, std::time::SystemTime)> {
    (1..=512)
        .map(|index| image_e01::segment_path(image, index))
        .take_while(|path| path.is_file())
        .map(|path| {
            let metadata = std::fs::metadata(&path).unwrap();
            (path, metadata.len(), metadata.modified().unwrap())
        })
        .collect()
}
