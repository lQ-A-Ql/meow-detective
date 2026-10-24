//! Opt-in service/Docker acceptance and Release timing against real Linux E01 evidence.
use app_services::{
    analysis_service::{self, AnalysisExtractionProgressContext, AnalysisSourceReadRuntime},
    file_service,
    import_analysis::ImportAnalysisMode,
    import_pipeline::{execute_import_job_profiled, ImportJobOptions, ImportProfiler},
};
use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};
use rusqlite::Connection;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
    time::Instant,
};

#[path = "support/linux_service_docker_acceptance.rs"]
mod acceptance;

#[test]
#[ignore = "requires FORENSICS_LINUX_SERVICE_E01 and FORENSICS_LINUX_SERVICE_OUTPUT real evidence paths"]
fn linux_e01_service_docker_acceptance_and_performance() {
    let fixture = std::path::PathBuf::from(
        std::env::var_os("FORENSICS_LINUX_SERVICE_E01").expect("set FORENSICS_LINUX_SERVICE_E01"),
    );
    let output = std::path::PathBuf::from(
        std::env::var_os("FORENSICS_LINUX_SERVICE_OUTPUT")
            .expect("set FORENSICS_LINUX_SERVICE_OUTPUT"),
    );
    assert!(fixture.is_file(), "E01 fixture must exist");
    assert!(
        !output.exists(),
        "each acceptance run needs a new output directory"
    );
    std::fs::create_dir_all(&output).unwrap();
    let original = std::fs::metadata(&fixture).unwrap();
    let reused_case = std::env::var_os("FORENSICS_LINUX_SERVICE_CASE_ROOT");
    let active = match reused_case.as_ref() {
        Some(root) => app_services::case_service::open_case(Path::new(root)).unwrap(),
        None => {
            app_services::case_service::create_case(&output, "linux-services-docker", None).unwrap()
        }
    };
    let config = app_services::import_precheck::prepare_import_source_config_from_path(
        fixture.to_str().unwrap(),
        domain::DataSourcePlatform::Linux,
    )
    .unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let profiler = ImportProfiler::default();
    let sink = acceptance::ProgressSink::default();
    let import_started = Instant::now();
    let source_id = active
        .with_conn(|connection| {
            if reused_case.is_some() {
                let sources = DataSourceRepo::new(connection).find_by_case(&active.meta.id)?;
                assert_eq!(sources.len(), 1);
                assert_eq!(
                    sources[0].source_path.canonicalize().unwrap(),
                    fixture.canonicalize().unwrap()
                );
                eprintln!("ACCEPTANCE_PHASE reusing retained metadata catalog");
                return Ok(sources[0].id.clone());
            }
            let job = JobRepo::new(connection)
                .create(&active.meta.id.0, "Linux service sample acceptance")?;
            JobRepo::new(connection).start(&job)?;
            eprintln!(
                "ACCEPTANCE_PHASE import started sample={}",
                fixture.file_name().unwrap().to_string_lossy()
            );
            let outcome = execute_import_job_profiled(
                connection,
                &active.meta.id,
                &active.case_root,
                config,
                &job,
                ImportJobOptions {
                    projection_policy: Default::default(),
                    event_sink: Some(&sink),
                    cancel_token: &cancel,
                    max_import_workers: Some(1),
                    max_analysis_workers: Some(1),
                    analysis_mode: ImportAnalysisMode::MetadataOnly,
                    defer_linux_artifacts: true,
                },
                &profiler,
            );
            acceptance::write_json(
                &output.join("import-profile.json"),
                &serde_json::to_value(profiler.report()).unwrap(),
            );
            let (message, _) =
                outcome.map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            JobRepo::new(connection).complete_committed(&job, &message)?;
            let sources = DataSourceRepo::new(connection).find_by_case(&active.meta.id)?;
            assert_eq!(sources.len(), 1);
            Ok(sources[0].id.clone())
        })
        .unwrap();
    let import_ms = reused_case
        .is_none()
        .then(|| import_started.elapsed().as_secs_f64() * 1000.0);
    eprintln!(
        "ACCEPTANCE_PHASE import completed elapsed_ms={import_ms:?} source={}",
        source_id.0
    );
    active.with_conn(|connection| {
        let source = app_services::source_db::open_registered_source_db(connection, &active.case_root, &source_id)
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
        acceptance::write_json(&output.join("catalog.json"), &acceptance::catalog(&source));
        let mut trials = Vec::new();
        let mut expected_digest = std::env::var("FORENSICS_LINUX_SERVICE_EXPECTED_DIGEST").ok();
        for trial in 0..=5 {
            source.execute_batch("DELETE FROM artifacts; DELETE FROM timeline_events; DELETE FROM source_meta WHERE key LIKE 'analysis_candidate_scan:%';")?;
            let started = Instant::now();
            let runtime = AnalysisSourceReadRuntime::default();
            let context = AnalysisExtractionProgressContext::new(&runtime, "linux-service-acceptance");
            let counters = acceptance::process_counters();
            let run = analysis_service::run_source_analysis_extraction_with_progress(
                connection, &active.case_root, &active.meta.id, &source_id,
                &["LinuxSystemConfig"], context,
                |progress| sink.analysis(&progress),
            ).map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
            let read_transfer_bytes = acceptance::process_counters().0.saturating_sub(counters.0);
            let digest = acceptance::artifact_digest(&source);
            if let Some(expected) = &expected_digest { assert_eq!(&digest, expected, "repeated extraction must preserve output"); }
            expected_digest = Some(digest.clone());
            assert!(run.scanned_count > 0);
            assert_eq!(run.checkpoint_hit_count, 0, "fresh runs must perform extraction");
            let record = json!({
                "trial":trial,"warmup":trial==0,"elapsedMs":elapsed_ms,
                "scannedCount":run.scanned_count,"artifactCount":run.artifact_count,
                "warningCount":run.warnings.len(),"outputSha256":digest,
                "processReadTransferBytes":read_transfer_bytes,
                "peakRssMiB":app_services::runtime_resources::peak_rss_mb(),
            });
            acceptance::write_json(&output.join(format!("extraction-{trial}.json")), &json!({"measurement":record,"run":run}));
            eprintln!("ACCEPTANCE_EXTRACTION {record}");
            trials.push(record);
        }
        let verified = verify_views(connection, &source, &active.case_root, &active.meta.id, &source_id);
        let replay_started = Instant::now();
        let replay = analysis_service::run_source_analysis_extraction(connection, &active.case_root, &active.meta.id, &source_id, &["LinuxSystemConfig"])
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
        let replay_ms = replay_started.elapsed().as_secs_f64()*1000.0;
        assert_eq!(acceptance::artifact_digest(&source), expected_digest.unwrap());
        assert!(replay.checkpoint_hit_count > 0);
        assert!(app_services::runtime_resources::peak_rss_mb() < 2048);
        let summary = json!({
            "sample":fixture.file_name().unwrap().to_string_lossy(),"caseRoot":active.case_root,
            "sourceId":source_id.0,"release":!cfg!(debug_assertions),
            "importElapsedMs":import_ms,"trials":trials,"verified":verified,
            "replayElapsedMs":replay_ms,
            "replayCheckpointHits":replay.checkpoint_hit_count,
            "peakRssMiB":app_services::runtime_resources::peak_rss_mb(),
            "scope":"metadata-only production import; LinuxSystemConfig extraction; source-scoped summaries and service text preview",
            "observationLimits":["OS transfer counters include cached IO","import timing is one observation; extraction has one warmup and five measurements","RSS peak covers the test process lifetime including import"],
        });
        acceptance::write_json(&output.join("summary.json"), &summary);
        eprintln!("ACCEPTANCE_RESULT {summary}");
        Ok(())
    }).unwrap();
    let after = std::fs::metadata(&fixture).unwrap();
    assert_eq!(original.len(), after.len());
    assert_eq!(original.modified().unwrap(), after.modified().unwrap());
}

fn verify_views(
    case: &Connection,
    source: &Connection,
    root: &Path,
    case_id: &CaseId,
    source_id: &DataSourceId,
) -> Value {
    let mut offset = 0;
    let mut services = Vec::new();
    let mut containers = Vec::new();
    let mut images = Vec::new();
    loop {
        let summary =
            analysis_service::get_source_linux_summary(case, root, case_id, source_id, offset, 200)
                .unwrap();
        for row in &summary.system_configs {
            match row.config_kind.as_str() {
                "systemdServiceUnit"
                    if row
                        .name
                        .as_deref()
                        .is_some_and(|name| name.ends_with(".service")) =>
                {
                    services.push(row.clone())
                }
                "dockerContainer" => containers.push(row.clone()),
                "dockerImage" => images.push(row.clone()),
                _ => {}
            }
        }
        offset += summary.system_configs.len() as u64;
        if offset >= summary.system_config_count {
            break;
        }
        assert!(
            !summary.system_configs.is_empty(),
            "summary paging must make progress"
        );
    }
    assert!(
        !services.is_empty(),
        "real Linux sample must expose configured services"
    );
    let unit = services
        .iter()
        .find(|unit| !unit.exec_start.is_empty() && unit.unit_file.is_some())
        .expect("service with parsed ExecStart");
    assert!(unit.file_id.starts_with(&format!("ds:{}:", source_id.0)));
    let preview_started = Instant::now();
    let preview = file_service::text_preview_for_source_case(
        case,
        root,
        case_id,
        &unit.file_id,
        Some(1024 * 1024),
    )
    .unwrap();
    let preview_ms = preview_started.elapsed().as_secs_f64() * 1000.0;
    assert!(preview.content.contains("[Service]"));
    assert!(preview.content.contains("ExecStart="));
    let raw_containers: u64 = source.query_row("SELECT COUNT(*) FROM file_entries WHERE ('/'||path) LIKE '%/var/lib/docker/containers/%/config.v2.json'", [], |row|row.get(0)).unwrap();
    let raw_images: u64 = source.query_row("SELECT COUNT(*) FROM file_entries WHERE ('/'||path) LIKE '%/var/lib/docker/image/%/repositories.json'", [], |row|row.get(0)).unwrap();
    let containerd_stores: u64 = source.query_row("SELECT COUNT(*) FROM file_entries WHERE path LIKE '%/io.containerd.metadata.v1.bolt/meta.db'", [], |row|row.get(0)).unwrap();
    assert_eq!(
        containers.len() as u64,
        raw_containers,
        "every container metadata file must reach the summary"
    );
    if raw_images > 0 || containerd_stores > 0 {
        assert!(
            !images.is_empty(),
            "real image repositories must reach the summary"
        );
    }
    let enabled = services
        .iter()
        .filter(|row| row.enabled == Some(true))
        .count();
    let enablement_links: u64 = source.query_row("SELECT COUNT(*) FROM file_entries WHERE (('/'||path) LIKE '/etc/systemd/system/%.wants/%.service' OR ('/'||path) LIKE '/etc/systemd/system/%.requires/%.service') AND (unix_mode & 61440)=40960", [], |row|row.get(0)).unwrap();
    if enablement_links > 0 {
        assert!(
            enabled > 0,
            "source enablement links must produce enabled services"
        );
    }
    json!({"services":services.len(),"enabledServices":enabled,"hostEnablementLinks":enablement_links,
        "containers":containers.len(),"images":images.len(),"containerMetadataFiles":raw_containers,"imageRepositoryFiles":raw_images,"containerdStores":containerd_stores,
        "servicePreview":{"fileId":unit.file_id,"path":unit.source_path,"elapsedMs":preview_ms,
        "bytes":preview.content.len(),"sha256":hex::encode(Sha256::digest(preview.content.as_bytes()))},
        "containerRows":containers,"imageRows":images})
}
