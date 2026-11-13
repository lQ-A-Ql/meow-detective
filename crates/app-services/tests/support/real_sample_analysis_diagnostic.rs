use app_services::{active_case::ActiveCase, analysis_service, case_service, source_db};
use domain::{DataSourceId, DataSourcePlatform};
use persistence_source::fjall::{CatalogRecordStore, ScanDirection};
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};

pub(super) fn run() {
    let (active, source, platform, output) = open_diagnostic();
    let source_id = source.id.clone();
    let case_root = active.case_root.clone();
    write_registry_metadata(&active, &source_id, &output);
    let run_id = uuid::Uuid::new_v4().to_string();
    let categories: &[&str] = match platform {
        DataSourcePlatform::Windows => &["Registry", "BrowserHistory", "Email", "EventLogs"],
        DataSourcePlatform::Linux => &["LinuxArtifacts"],
        _ => panic!("diagnostic requires a Windows or Linux source"),
    };
    let before = source_bindings(&active);
    let input_before = std::fs::metadata(&source.source_path).unwrap();
    super::write_report(
        &output.join("diagnostic-context.json"),
        &json!({"caseRoot":case_root, "caseId":active.meta.id.0,
            "sourceId":source_id.0, "platform":platform.as_storage_str(),
            "categories":categories, "runId":run_id, "bindingsBefore":before}),
    );
    run_diagnostic(
        &active,
        &source,
        &output,
        &run_id,
        categories,
        before,
        input_before,
    );
}

pub(super) fn dump_registry_metadata() {
    let (active, source, _, output) = open_diagnostic();
    write_registry_metadata(&active, &source.id, &output);
}

fn open_diagnostic() -> (ActiveCase, domain::DataSource, DataSourcePlatform, PathBuf) {
    let case_root = std::fs::canonicalize(PathBuf::from(
        std::env::var_os("FORENSICS_ANALYSIS_CASE_ROOT").expect("set FORENSICS_ANALYSIS_CASE_ROOT"),
    ))
    .unwrap();
    let source_id = DataSourceId(
        std::env::var("FORENSICS_ANALYSIS_SOURCE_ID").expect("set FORENSICS_ANALYSIS_SOURCE_ID"),
    );
    let active = case_service::open_case(&case_root).unwrap();
    let (source, platform) = selected_source(&active, &source_id);
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let configured_output = PathBuf::from(
        std::env::var_os("FORENSICS_PERF_OUTPUT").expect("set FORENSICS_PERF_OUTPUT"),
    )
    .canonicalize()
    .unwrap();
    assert!(
        configured_output.starts_with(workspace),
        "output must be in workspace"
    );
    let output = super::output_directory(platform);
    (active, source, platform, output)
}

fn run_diagnostic(
    active: &ActiveCase,
    source: &domain::DataSource,
    output: &std::path::Path,
    run_id: &str,
    categories: &[&str],
    before: BTreeMap<String, String>,
    input_before: std::fs::Metadata,
) {
    let source_id = &source.id;
    let case_root = &active.case_root;
    let mut journal = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output.join("diagnostic-progress.jsonl"))
        .unwrap();
    let runtime = analysis_service::AnalysisSourceReadRuntime::default();
    let memory = super::RssSampler::start();
    let started = Instant::now();
    let mut last_output = Instant::now() - Duration::from_secs(2);
    let mut progress_events = 0u64;
    let result = active
        .with_conn(|connection| {
            Ok(analysis_service::run_import_source_extraction(
                analysis_service::ImportSourceExtractionRequest {
                    case_conn: connection,
                    case_root: &active.case_root,
                    case_id: &active.meta.id,
                    source: source_id,
                    categories,
                    runtime: &runtime,
                    cancel_token: Arc::new(AtomicBool::new(false)),
                    run_id,
                },
                |update| {
                    assert_eq!(update.case_id, active.meta.id.0);
                    assert_eq!(update.data_source_id, source_id.0);
                    serde_json::to_writer(&mut journal, &update).unwrap();
                    writeln!(&mut journal).unwrap();
                    journal.flush().unwrap();
                    progress_events += 1;
                    if last_output.elapsed() >= Duration::from_secs(2)
                        || update.work.as_ref().is_some_and(|work| work.complete)
                    {
                        eprintln!(
                            "ANALYSIS_DIAGNOSTIC phase={:?} category={} processed={} total={} warnings={} work={:?}",
                            update.phase, update.category, update.processed_candidates,
                            update.total_candidates, update.warning_candidates, update.work
                        );
                        last_output = Instant::now();
                    }
                },
            ))
        })
        .unwrap();
    let elapsed_ms = started.elapsed().as_millis();
    let memory = memory.finish();
    journal.sync_all().unwrap();
    let after = source_bindings(active);
    let input_after = std::fs::metadata(&source.source_path).unwrap();
    let unchanged = input_before.len() == input_after.len()
        && input_before.modified().unwrap() == input_after.modified().unwrap()
        && input_before.permissions().readonly() == input_after.permissions().readonly();
    let report = json!({"schemaVersion":1, "caseRoot":case_root,
        "caseId":active.meta.id.0, "sourceId":source_id.0,
        "runId":run_id,
        "categories":categories, "elapsedMs":elapsed_ms, "memory":memory,
        "progressEvents":progress_events, "inputMetadataUnchanged":unchanged,
        "bindingsBefore":before, "bindingsAfter":after,
        "execution":execution_report(&result)});
    let report_path = output.join("analysis-diagnostic.json");
    super::write_report(&report_path, &report);
    eprintln!("ANALYSIS_DIAGNOSTIC_REPORT {}", report_path.display());
    assert!(unchanged, "source input metadata changed");
    for (id, binding) in &before {
        if id != &source_id.0 {
            assert_eq!(
                after.get(id),
                Some(binding),
                "foreign source binding changed"
            );
        }
    }
    assert_eq!(before.len(), after.len(), "source membership changed");
    let execution = result.expect("diagnostic API failed; report retained");
    assert_eq!(
        execution.retryable_failure_count, 0,
        "analysis failures; report retained"
    );
}

fn write_registry_metadata(
    active: &ActiveCase,
    source_id: &DataSourceId,
    output: &std::path::Path,
) {
    let records = active
        .with_conn(|connection| {
            let sql = source_db::open_registered_source_db_read_only(
                connection,
                &active.case_root,
                source_id,
            )?;
            let store = persistence_source::detail::require_source_store(&sql)?;
            let catalog = CatalogRecordStore::from_source_store((*store).clone());
            let snapshot = catalog.read_snapshot();
            let mut cursor = None;
            let mut records = Vec::new();
            loop {
                let page = catalog
                    .list_all(
                        &snapshot,
                        &source_id.0,
                        cursor.as_deref(),
                        ScanDirection::Forward,
                        512,
                    )
                    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
                for record in page.records {
                    assert_eq!(record.data_source_id, source_id.0);
                    let name = record.name.to_ascii_lowercase();
                    let base = name
                        .strip_suffix(".log1")
                        .or_else(|| name.strip_suffix(".log2"))
                        .unwrap_or(&name);
                    if matches!(
                        base,
                        "ntuser.dat"
                            | "usrclass.dat"
                            | "amcache.hve"
                            | "system"
                            | "sam"
                            | "security"
                            | "software"
                            | "default"
                            | "components"
                    ) {
                        records.push(record);
                    }
                }
                cursor = page.next_cursor;
                if cursor.is_none() {
                    break;
                }
            }
            records.sort_by(|left, right| {
                left.path
                    .cmp(&right.path)
                    .then_with(|| left.id.cmp(&right.id))
            });
            Ok(records)
        })
        .unwrap();
    let path = output.join("registry-catalog.json");
    super::write_report(
        &path,
        &json!({"caseId":active.meta.id.0, "sourceId":source_id.0, "records":records}),
    );
    eprintln!("REGISTRY_CATALOG_REPORT {}", path.display());
}

fn selected_source(
    active: &ActiveCase,
    source_id: &DataSourceId,
) -> (domain::DataSource, DataSourcePlatform) {
    active
        .with_conn(|connection| {
            assert!(
                JobRepo::new(connection).find_interrupted()?.is_empty(),
                "case has active jobs"
            );
            let repository = DataSourceRepo::new(connection);
            let source = repository
                .find_by_case(&active.meta.id)?
                .into_iter()
                .find(|source| source.id == *source_id)
                .expect("source does not belong to case");
            let storage = repository
                .find_storage(source_id)?
                .expect("source storage missing");
            assert!(matches!(
                storage.import_state.as_str(),
                "ready" | "ready_metadata"
            ));
            let platform = match storage.platform.as_str() {
                "windows" => DataSourcePlatform::Windows,
                "linux" => DataSourcePlatform::Linux,
                _ => panic!("diagnostic requires a Windows or Linux source"),
            };
            let sql = source_db::open_registered_source_db_read_only(
                connection,
                &active.case_root,
                source_id,
            )?;
            assert_eq!(
                persistence_source::detail::source_id(&sql)?.as_deref(),
                Some(source_id.0.as_str())
            );
            Ok((source, platform))
        })
        .unwrap()
}

fn source_bindings(active: &ActiveCase) -> BTreeMap<String, String> {
    active
        .with_conn(|connection| {
            DataSourceRepo::new(connection)
                .find_by_case(&active.meta.id)?
                .into_iter()
                .map(|source| {
                    let sql = source_db::open_registered_source_db_read_only(
                        connection,
                        &active.case_root,
                        &source.id,
                    )?;
                    let binding = sql.query_row(
                        "SELECT value FROM source_meta WHERE key='fjall_root'",
                        [],
                        |row| row.get(0),
                    )?;
                    Ok((source.id.0, binding))
                })
                .collect()
        })
        .unwrap()
}

fn execution_report(
    result: &Result<
        analysis_service::AnalysisExtractionExecution,
        analysis_service::AnalysisServiceError,
    >,
) -> serde_json::Value {
    match result {
        Ok(execution) => json!({
            "status":if execution.retryable_failure_count > 0 { "failed" }
                else if execution.dto.warnings.is_empty() { "passed" } else { "passedWithWarnings" },
            "dto":execution.dto, "retryableFailureCount":execution.retryable_failure_count,
            "discoveryElapsedMs":execution.discovery_elapsed_ms,
            "processingElapsedMs":execution.processing_elapsed_ms,
            "persistenceElapsedMs":execution.persistence_elapsed_ms,
            "sourceReadCount":execution.source_read_count,
            "sourceReadElapsedMs":execution.source_read_elapsed_ms,
            "workMetrics":execution.work_metrics,
            "pluginFailureCount":execution.plugin_extract_failures.len()
        }),
        Err(error) => json!({"status":"failed", "error":error.to_string()}),
    }
}
