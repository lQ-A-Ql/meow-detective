#[path = "support/import_ntfs_fixture.rs"]
mod fixture;
#[path = "support/import_benchmark_oracle_tests.rs"]
mod oracle_tests;
#[path = "support/import_projection_policy.rs"]
mod projection_policy;
#[path = "support/import_benchmark_queries.rs"]
mod queries;
#[path = "support/import_ntfs_scenarios.rs"]
mod scenarios;
#[path = "support/import_benchmark_semantics.rs"]
mod semantics;
#[path = "support/import_benchmark_stream.rs"]
mod streaming;

use app_services::import_pipeline::ImportEventSink;
use app_services::{
    case_service,
    import_analysis::ImportAnalysisMode,
    import_pipeline::{
        execute_import_job_profiled, execute_import_job_with_counts, ImportJobOptions,
        ImportProfiler,
    },
    import_precheck, source_db,
};
use domain::{DataSourceId, DataSourcePlatform, EntryType, FileEntry, FileEntryId};
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::repositories::job_repo::JobRepo;
use rusqlite::{types::Value, Connection};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;
use std::sync::{atomic::AtomicBool, Arc};
use transport::dto::import_benchmark::{
    ImportBenchmarkPhaseDto as Phase, ImportBenchmarkReportDto,
};
use transport::dto::{
    DataSourceSummaryDto, ImportPhaseProgressDto, IndexCacheStatusDto, JobCancellationDto,
    PartialResultDto,
};

// Correctness comparisons must not compete with another test's import for the
// process-wide resource pool while collecting preview/extraction oracles.
// Reader workers inside each workflow retain their requested concurrency.
static IMPORT_ORACLE_WINDOW: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct BenchmarkPhaseSink;

impl ImportEventSink for BenchmarkPhaseSink {
    fn job_progress(&self, _job_id: &str, _progress: u32, _detail: &str) {}
    fn partition_progress(
        &self,
        _job_id: &str,
        _current_partition: &str,
        _completed: u32,
        _total: u32,
        _partition_pct: u32,
    ) {
    }
    fn timeline_updated(&self, _event_count: u64) {}
    fn search_index_progress(&self, _progress: u32, _detail: &str) {}
    fn data_source_imported(
        &self,
        _case_id: &str,
        _data_source: &DataSourceSummaryDto,
        _job_id: &str,
    ) {
    }
    fn import_phase_progress(&self, progress: &ImportPhaseProgressDto) {
        eprintln!("[import-phase] {}", progress.detail);
    }
    fn import_partial_result(&self, _result: &PartialResultDto) {}
    fn cache_index_status(&self, _status: &IndexCacheStatusDto) {}
    fn job_cancellation(&self, _cancellation: &JobCancellationDto) {}
}

struct ImportResult {
    catalog: Vec<Vec<Value>>,
    graph: Vec<Vec<Value>>,
    timeline: Vec<Vec<Value>>,
    report: ImportBenchmarkReportDto,
}

struct CatalogFixture {
    _root: tempfile::TempDir,
    conn: Connection,
}

fn import(path: &Path, profiling: bool) -> ImportResult {
    import_with_workers(path, profiling, 1)
}

fn import_with_workers(path: &Path, profiling: bool, workers: usize) -> ImportResult {
    with_import(
        path,
        profiling,
        workers,
        |source, case_id, source_id, report| {
            let replacements = canonical_replacements(source, case_id, source_id);
            let catalog = native_catalog_snapshot(source, &replacements);
            let mut graph = snapshot(source, queries::NODES, &replacements);
            graph.extend(snapshot(source, queries::EDGES, &replacements));
            let timeline = native_timeline_snapshot(source, &replacements);
            ImportResult {
                catalog,
                graph,
                timeline,
                report,
            }
        },
    )
}

fn with_import<T>(
    path: &Path,
    profiling: bool,
    workers: usize,
    capture: impl FnOnce(&Connection, &str, &str, ImportBenchmarkReportDto) -> T,
) -> T {
    with_import_context(
        path,
        profiling,
        workers,
        ImportAnalysisMode::MetadataOnly,
        |source, _, _, case, id, report| capture(source, case, id, report),
    )
}

fn with_import_context<T>(
    path: &Path,
    profiling: bool,
    workers: usize,
    mode: ImportAnalysisMode,
    capture: impl FnOnce(&Connection, &Connection, &Path, &str, &str, ImportBenchmarkReportDto) -> T,
) -> T {
    let _oracle_window = IMPORT_ORACLE_WINDOW
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let temporary = tempfile::TempDir::new().unwrap();
    let active = case_service::create_case(
        &temporary.path().join("cases"),
        "synthetic-benchmark",
        Some("tester"),
    )
    .unwrap();
    let captured = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        active
            .with_conn(|conn| {
                let job = JobRepo::new(conn).create(&active.meta.id.0, "Synthetic benchmark")?;
                let config = import_precheck::prepare_import_source_config_from_path(
                    &path.to_string_lossy(),
                    DataSourcePlatform::Windows,
                )
                .unwrap();
                let cancel = Arc::new(AtomicBool::new(false));
                let phase_sink = BenchmarkPhaseSink;
                let phase_trace = std::env::var_os("FORENSICS_IMPORT_BENCH_PHASE_TRACE").is_some();
                let options = ImportJobOptions {
                    projection_policy: projection_policy::requested(),
                    event_sink: phase_trace.then_some(&phase_sink as &dyn ImportEventSink),
                    cancel_token: &cancel,
                    max_import_workers: Some(workers),
                    max_analysis_workers: Some(1),
                    analysis_mode: mode,
                    defer_linux_artifacts: false,
                };
                let profiler = ImportProfiler::default();
                let result = if profiling {
                    execute_import_job_profiled(
                        conn,
                        &active.meta.id,
                        &active.case_root,
                        config,
                        &job,
                        options,
                        &profiler,
                    )
                } else {
                    execute_import_job_with_counts(
                        conn,
                        &active.meta.id,
                        &active.case_root,
                        config,
                        &job,
                        options,
                    )
                };
                result.unwrap_or_else(|error| panic!("synthetic import failed: {error}"));
                // Capture import RSS before the correctness oracle allocates snapshots.
                let report = profiler.report();
                let (source_id, state): (String, String) =
                    conn.query_row("SELECT id, import_state FROM data_sources", [], |row| {
                        Ok((row.get(0)?, row.get(1)?))
                    })?;
                assert_eq!(state, "ready");
                let source = source_db::open_registered_source_db_read_only(
                    conn,
                    &active.case_root,
                    &DataSourceId(source_id.clone()),
                )?;
                Ok(capture(
                    &source,
                    conn,
                    &active.case_root,
                    &active.meta.id.0,
                    &source_id,
                    report,
                ))
            })
            .unwrap()
    }));
    match captured {
        Ok(value) => value,
        Err(error) => {
            drop(active);
            eprintln!("[failed-case-retained] {}", temporary.keep().display());
            std::panic::resume_unwind(error)
        }
    }
}

fn canonical_replacements(
    conn: &Connection,
    case_id: &str,
    source_id: &str,
) -> Vec<(String, String)> {
    let records = native_catalog_records(conn);
    let mut roots = records
        .iter()
        .filter(|record| record.parent_id.is_none())
        .map(|record| record.id.clone())
        .collect::<Vec<_>>();
    roots.sort();
    let mut replacements = vec![
        (source_id.into(), "__source__".into()),
        (case_id.into(), "__case__".into()),
    ];
    replacements.extend(
        roots
            .into_iter()
            .enumerate()
            .map(|(i, id)| (id, format!("__root_{i}__"))),
    );
    let mut identities = std::collections::HashSet::new();
    let mut replacement_bytes = 0usize;
    for record in records.iter().filter(|record| record.parent_id.is_some()) {
        let id = &record.id;
        let partition = record.partition_index.map(i64::from).unwrap_or(-1);
        let path = &record.path;
        if uuid::Uuid::parse_str(id).is_ok_and(|id| id.get_version_num() == 4) {
            let stable = format!("__file_{}:{}:{}__", partition, path.len(), path);
            assert!(
                identities.insert(stable.clone()),
                "ambiguous generated-file identity in benchmark oracle"
            );
            replacement_bytes += id.len() + stable.len() + 128;
            assert!(
                replacements.len() < 100_000 && replacement_bytes <= 64 * 1024 * 1024,
                "benchmark UUID normalization map exceeds its 64 MiB / 100k-entry limit"
            );
            replacements.push((id.clone(), stable));
        }
    }
    replacements
}

fn native_catalog_records(conn: &Connection) -> Vec<persistence_source::fjall::FileEntryRecord> {
    let source_id = persistence_source::detail::source_id(conn)
        .unwrap()
        .expect("source identity");
    let store = persistence_source::detail::require_source_store(conn).unwrap();
    let repository =
        persistence_source::fjall::CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = repository.read_snapshot();
    let mut cursor = None;
    let mut records = Vec::new();
    loop {
        let page = repository
            .list_all(
                &snapshot,
                &source_id,
                cursor.as_deref(),
                persistence_source::fjall::ScanDirection::Forward,
                512,
            )
            .unwrap();
        records.extend(page.records);
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    records
}

fn native_catalog_snapshot(
    conn: &Connection,
    replacements: &[(String, String)],
) -> Vec<Vec<Value>> {
    let rows = native_catalog_records(conn)
        .into_iter()
        .map(|record| {
            vec![
                Value::Text(record.id),
                record.parent_id.map(Value::Text).unwrap_or(Value::Null),
                Value::Text(record.path),
                Value::Text(record.name),
                Value::Text(
                    match record.entry_type {
                        persistence_source::fjall::CatalogEntryType::File => "file",
                        persistence_source::fjall::CatalogEntryType::Directory => "directory",
                    }
                    .into(),
                ),
                record
                    .size
                    .map(|value| Value::Integer(value as i64))
                    .unwrap_or(Value::Null),
                record.ext.map(Value::Text).unwrap_or(Value::Null),
                Value::Integer(record.deleted as i64),
                Value::Integer(record.hidden as i64),
                Value::Integer(record.system as i64),
                Value::Integer(record.read_only as i64),
                Value::Integer(record.archive as i64),
                record
                    .unix_mode
                    .map(|value| Value::Integer(value as i64))
                    .unwrap_or(Value::Null),
                record
                    .encrypted
                    .map(|value| Value::Integer(value as i64))
                    .unwrap_or(Value::Null),
                record
                    .created
                    .map(|value| Value::Text(value.to_rfc3339()))
                    .unwrap_or(Value::Null),
                record
                    .modified
                    .map(|value| Value::Text(value.to_rfc3339()))
                    .unwrap_or(Value::Null),
                record
                    .accessed
                    .map(|value| Value::Text(value.to_rfc3339()))
                    .unwrap_or(Value::Null),
                record
                    .changed
                    .map(|value| Value::Text(value.to_rfc3339()))
                    .unwrap_or(Value::Null),
                record.hash_sha256.map(Value::Text).unwrap_or(Value::Null),
                record
                    .partition_index
                    .map(|value| Value::Integer(value as i64))
                    .unwrap_or(Value::Null),
            ]
        })
        .collect();
    snapshot_rows(rows, replacements)
}

fn native_timeline_snapshot(
    conn: &Connection,
    replacements: &[(String, String)],
) -> Vec<Vec<Value>> {
    let source_id = persistence_source::detail::source_id(conn)
        .unwrap()
        .expect("source identity");
    let store = persistence_source::detail::require_source_store(conn).unwrap();
    let repository =
        persistence_source::fjall::TimelineRecordStore::from_source_store((*store).clone());
    let snapshot = repository.read_snapshot();
    let mut cursor = None;
    let mut records = Vec::new();
    loop {
        let page = repository
            .list_by_source(
                &snapshot,
                &source_id,
                cursor.as_deref(),
                persistence_source::fjall::ScanDirection::Forward,
                512,
            )
            .unwrap();
        records.extend(page.records);
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    let rows = records
        .into_iter()
        .map(|record| {
            vec![
                Value::Text(record.id),
                Value::Text(record.source_object_id),
                Value::Text(record.event_type),
                Value::Text(record.timestamp.to_rfc3339()),
                Value::Text(record.title),
                Value::Text(record.description),
                record.parser_id.map(Value::Text).unwrap_or(Value::Null),
                record
                    .parser_version
                    .map(Value::Text)
                    .unwrap_or(Value::Null),
                record
                    .confidence
                    .map(|value| Value::Real(value as f64))
                    .unwrap_or(Value::Null),
                record
                    .source_attribution
                    .map(Value::Text)
                    .unwrap_or(Value::Null),
                Value::Text(serde_json::to_string(&record.attrs).unwrap()),
            ]
        })
        .collect();
    snapshot_rows(rows, replacements)
}

fn native_edges_snapshot(conn: &Connection, replacements: &[(String, String)]) -> Vec<Vec<Value>> {
    let rows = native_catalog_records(conn)
        .into_iter()
        .filter_map(|record| {
            let parent_id = record.parent_id?;
            Some(vec![
                Value::Text(format!("contains:{parent_id}:{}", record.id)),
                Value::Text(record.id),
            ])
        })
        .collect();
    snapshot_rows(rows, replacements)
}

fn snapshot(conn: &Connection, sql: &str, replacements: &[(String, String)]) -> Vec<Vec<Value>> {
    let mut statement = conn.prepare(sql).unwrap();
    let columns = statement.column_count();
    let rows: Vec<Vec<Value>> = statement
        .query_map([], |row| (0..columns).map(|index| row.get(index)).collect())
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    snapshot_rows(rows, replacements)
}

fn snapshot_rows(mut rows: Vec<Vec<Value>>, replacements: &[(String, String)]) -> Vec<Vec<Value>> {
    let identities: std::collections::HashMap<_, _> = replacements
        .iter()
        .map(|(id, stable)| (id.as_str(), stable.as_str()))
        .collect();
    let uuid =
        regex::Regex::new(r"(?i)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
            .unwrap();
    for row in &mut rows {
        for value in row {
            if let Value::Text(text) = value {
                if let std::borrow::Cow::Owned(normalized) =
                    uuid.replace_all(text, |capture: &regex::Captures<'_>| {
                        identities
                            .get(&capture[0])
                            .copied()
                            .unwrap_or(&capture[0])
                            .to_owned()
                    })
                {
                    *text = normalized;
                }
            }
        }
    }
    rows.sort_by_cached_key(|row| format!("{row:?}"));
    rows
}

fn verify_report(report: &ImportBenchmarkReportDto, record_count: u64) {
    let workflow = report
        .samples
        .iter()
        .find(|sample| sample.phase == Phase::Workflow)
        .unwrap();
    let process_usage = report
        .process_usage
        .as_ref()
        .expect("profiled workflow process window");
    assert_eq!(Some(process_usage.wall_time_ns), workflow.elapsed_ns);
    #[cfg(target_os = "windows")]
    {
        assert!(process_usage.user_cpu_ns.is_some());
        assert!(process_usage.kernel_cpu_ns.is_some());
        assert!(process_usage.process_read_transfer_bytes.is_some());
        assert!(process_usage.handles_at_start.is_some());
    }
    let path_sql = report
        .partition_aggregates
        .iter()
        .find(|sample| sample.phase == Phase::MftPathSql)
        .unwrap();
    assert_eq!(path_sql.main_table_updated_rows, Some(0));
    assert_eq!(path_sql.materialized_path_rows, Some(record_count - 14));
    let stage = report
        .partition_aggregates
        .iter()
        .find(|s| s.phase == Phase::MftStage)
        .unwrap();
    assert!(stage.writer_batches.is_some_and(|b| b > 0));
    assert!(stage.writer_batch_max_records.is_some_and(|n| n <= 10000));
    assert_eq!(stage.read_chunk_records, Some(10000));
    assert_eq!(
        report
            .partition_aggregates
            .iter()
            .find(|s| s.phase == Phase::MftCommit)
            .unwrap()
            .partition_commits,
        Some(1)
    );
    assert!(path_sql
        .staging_logical_bytes
        .is_some_and(|bytes| bytes > 0));
    let builds = report
        .partition_aggregates
        .iter()
        .find(|sample| sample.phase == Phase::ReaderBuild)
        .unwrap();
    assert_eq!(
        builds.filesystem_builds, 0,
        "direct NTFS must not build an eager filesystem"
    );
    let backfill = report
        .partition_aggregates
        .iter()
        .find(|sample| sample.phase == Phase::MftDirReaderBuild)
        .unwrap();
    assert_eq!(
        backfill.filesystem_builds, 1,
        "directory backfill still requires its NTFS reader"
    );
    assert!(!report
        .samples
        .iter()
        .any(|sample| sample.phase == Phase::FallbackReaderBuild));
    for phase in [
        Phase::Probe,
        Phase::Attach,
        Phase::SourceDatabaseOpen,
        Phase::SourceMetadataInitialize,
        Phase::RuntimePrepare,
        Phase::WorkspaceAdmission,
        Phase::Catalog,
        Phase::Finalize,
        Phase::OutcomePersistence,
        Phase::PostReady,
        Phase::Cleanup,
        Phase::Graph,
        Phase::Analysis,
        Phase::Timeline,
        Phase::TimelineEvents,
        Phase::TimelineGraph,
        Phase::MftReadParseWall,
        Phase::MftWriterWall,
        Phase::MftPathCompute,
        Phase::MftPathSql,
        Phase::TimelineEventClear,
        Phase::TimelineEventCursor,
        Phase::TimelineEventInsert,
        Phase::TimelineEventIndexes,
        Phase::TimelineEventCommit,
        Phase::TimelineGraphPage,
        Phase::TimelineGraphNodes,
        Phase::TimelineGraphEdges,
        Phase::TimelineGraphCommit,
        Phase::Checkpoint,
        Phase::StagingMerge,
        Phase::Workflow,
    ] {
        assert!(
            report
                .samples
                .iter()
                .any(|sample| sample.phase == phase && sample.elapsed_ns.is_some()),
            "missing {phase:?}"
        );
    }
    let root_phases = [
        Phase::Attach,
        Phase::SourceDatabaseOpen,
        Phase::SourceMetadataInitialize,
        Phase::RuntimePrepare,
        Phase::WorkspaceAdmission,
        Phase::Catalog,
        Phase::Analysis,
        Phase::Finalize,
        Phase::OutcomePersistence,
        Phase::PostReady,
        Phase::Cleanup,
    ];
    let covered: u64 = report
        .samples
        .iter()
        .filter(|sample| sample.partition_index.is_none() && root_phases.contains(&sample.phase))
        .map(|sample| sample.elapsed_ns.unwrap())
        .sum();
    let workflow = report
        .samples
        .iter()
        .find(|sample| sample.phase == Phase::Workflow)
        .unwrap();
    assert!(
        covered <= workflow.elapsed_ns.unwrap(),
        "root spans overlap or double-count work"
    );
    for phase in [
        Phase::MftReaderOpen,
        Phase::MftParams,
        Phase::MftRead,
        Phase::MftParse,
        Phase::MftStage,
        Phase::MftDirReaderBuild,
        Phase::MftSizeReconcile,
        Phase::MftDirBackfill,
        Phase::MftPathResolve,
        Phase::MftValidate,
        Phase::MftCommit,
    ] {
        assert!(
            report
                .partition_aggregates
                .iter()
                .any(|sample| sample.phase == phase),
            "missing {phase:?}"
        );
    }
    let read = report
        .partition_aggregates
        .iter()
        .find(|sample| sample.phase == Phase::MftRead)
        .unwrap();
    assert_eq!(read.scanned_records, record_count);
    assert_eq!(
        read.logical_bytes_read,
        record_count * fixture::RECORD_BYTES as u64
    );
    let stage = report
        .partition_aggregates
        .iter()
        .find(|sample| sample.phase == Phase::MftStage)
        .unwrap();
    assert_eq!(stage.staged_rows, record_count - 14);
    assert!(read.physical_bytes_read.is_none());
}

fn emit_benchmark(result: &ImportResult) {
    fn digest(rows: &[Vec<Value>]) -> String {
        let mut hasher = Sha256::new();
        for row in rows {
            let values: Vec<serde_json::Value> = row
                .iter()
                .map(|cell| match cell {
                    Value::Null => serde_json::json!(["null"]),
                    Value::Integer(n) => serde_json::json!(["integer", n]),
                    Value::Real(n) => serde_json::json!(["real", n]),
                    Value::Text(s) => serde_json::json!(["text", s]),
                    Value::Blob(b) => serde_json::json!(["blob", hex::encode(b)]),
                })
                .collect();
            hasher.update(serde_json::to_vec(&values).unwrap());
            hasher.update(b"\n");
        }
        hex::encode(hasher.finalize())
    }
    eprintln!(
        "[import-benchmark] {}",
        serde_json::json!({
            "schemaVersion": 1, "report": result.report,
            "catalogRows": result.catalog.len(), "catalogSha256": digest(&result.catalog),
            "graphRows": result.graph.len(), "graphSha256": digest(&result.graph),
            "timelineRows": result.timeline.len(), "timelineSha256": digest(&result.timeline),
            "oracleVersion": 1, "generatedIdsNormalized": true, "graphCreatedAtExcluded": true
        })
    );
}

#[test]
fn profiling_preserves_full_raw_catalog_graph_and_timeline() {
    let temporary = tempfile::TempDir::new().unwrap();
    let path = temporary.path().join("synthetic.raw");
    std::fs::write(&path, fixture::image(96)).unwrap();
    let original = import(&path, false);
    let observed = import(&path, true);
    assert_eq!(original.catalog, observed.catalog);
    assert_eq!(original.graph, observed.graph);
    assert_eq!(original.timeline, observed.timeline);
    assert!(original.report.samples.is_empty());
    verify_report(&observed.report, 96);
    let fs = fs_ntfs::NtfsReader::open(
        Box::new(evidence_core::RawImageReader::open(&path).unwrap()),
        0,
    )
    .unwrap();
    assert_eq!(
        fs.read_file_range_by_inode(16, 0, 12).unwrap(),
        fixture::FILE_CONTENT
    );
}

#[test]
fn compressed_e01_and_raw_produce_the_same_logical_catalog() {
    let temporary = tempfile::TempDir::new().unwrap();
    let raw_path = temporary.path().join("synthetic.raw");
    let e01_path = temporary.path().join("synthetic.E01");
    let bytes = fixture::image(96);
    std::fs::write(&raw_path, &bytes).unwrap();
    fixture::write_e01(&e01_path, &bytes).unwrap();
    let mut actual = Vec::new();
    image_e01::E01Reader::open(&e01_path)
        .unwrap()
        .read_to_end(&mut actual)
        .unwrap();
    assert_eq!(actual, bytes);
    let raw = import(&raw_path, true);
    let e01 = import(&e01_path, true);
    assert_eq!(raw.catalog, e01.catalog);
    assert_eq!(raw.graph, e01.graph);
    assert_eq!(raw.timeline, e01.timeline);
    verify_report(&e01.report, 96);
}

#[test]
fn mft_profile_counts_invalid_records_and_the_short_final_chunk() {
    let temporary = tempfile::TempDir::new().unwrap();
    let path = temporary.path().join("two-chunks.raw");
    std::fs::write(&path, fixture::image(10_019)).unwrap();
    verify_report(&import(&path, true).report, 10_019);
}

#[test]
fn oracle_normalizes_generated_generic_ids_and_preserves_native_mft_ids() {
    fn catalog() -> CatalogFixture {
        let fixture_root = tempfile::tempdir().unwrap();
        let source_id = DataSourceId("source".into());
        let conn =
            app_services::source_db::open_source_db(fixture_root.path(), &source_id).unwrap();
        conn.execute(
            "INSERT INTO data_sources
             (id, case_id, name, kind, source_path, imported_at)
             VALUES ('source', 'case', 'source', 'raw', 'source.raw',
                     '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        let root = uuid::Uuid::new_v4().to_string();
        let generated = uuid::Uuid::new_v4().to_string();
        let entries = [
            FileEntry {
                id: FileEntryId(root.clone()),
                parent_id: None,
                data_source_id: source_id.clone(),
                path: "[P1]".into(),
                name: "P1".into(),
                entry_type: EntryType::Directory,
                size: None,
                ext: None,
                deleted: false,
                hidden: false,
                system: false,
                encrypted: false,
                read_only: false,
                archive: false,
                unix_mode: None,
                created_at: None,
                modified_at: None,
                accessed_at: None,
                changed_at: None,
                hash_sha256: None,
            },
            FileEntry {
                id: FileEntryId(generated.clone()),
                parent_id: Some(FileEntryId(root.clone())),
                data_source_id: source_id.clone(),
                path: "[P1]/note.txt".into(),
                name: "note.txt".into(),
                entry_type: EntryType::File,
                size: Some(1),
                ext: Some("txt".into()),
                deleted: false,
                hidden: false,
                system: false,
                encrypted: false,
                read_only: false,
                archive: false,
                unix_mode: None,
                created_at: None,
                modified_at: None,
                accessed_at: None,
                changed_at: None,
                hash_sha256: None,
            },
            FileEntry {
                id: FileEntryId("mft:1:25".into()),
                parent_id: Some(FileEntryId(root.clone())),
                data_source_id: source_id.clone(),
                path: "[P1]/native.txt".into(),
                name: "native.txt".into(),
                entry_type: EntryType::File,
                size: Some(1),
                ext: Some("txt".into()),
                deleted: false,
                hidden: false,
                system: false,
                encrypted: false,
                read_only: false,
                archive: false,
                unix_mode: None,
                created_at: None,
                modified_at: None,
                accessed_at: None,
                changed_at: None,
                hash_sha256: None,
            },
        ];
        FileRepo::new(&conn).insert_batch(&entries).unwrap();
        for id in [root.as_str(), generated.as_str(), "mft:1:25"] {
            FileRepo::new(&conn)
                .set_partition_index_by_id(&FileEntryId(id.to_string()), 1)
                .unwrap();
        }
        CatalogFixture {
            _root: fixture_root,
            conn,
        }
    }
    let first = catalog();
    let second = catalog();
    let a = canonical_replacements(&first.conn, "case", "source");
    let b = canonical_replacements(&second.conn, "case", "source");
    assert_eq!(
        native_catalog_snapshot(&first.conn, &a),
        native_catalog_snapshot(&second.conn, &b)
    );
    assert_eq!(
        native_edges_snapshot(&first.conn, &a),
        native_edges_snapshot(&second.conn, &b)
    );
    assert!(native_catalog_snapshot(&first.conn, &a)
        .iter()
        .any(|row| row.contains(&Value::Text("mft:1:25".into()))));
    let store = persistence_source::detail::require_source_store(&second.conn).unwrap();
    let repository =
        persistence_source::fjall::CatalogRecordStore::from_source_store((*store).clone());
    let snapshot_value = repository.read_snapshot();
    let mut record = repository
        .get(&snapshot_value, "source", "mft:1:25")
        .unwrap()
        .unwrap();
    repository
        .delete(
            "source",
            "mft:1:25",
            persistence_source::fjall::PersistMode::SyncAll,
        )
        .unwrap();
    record.id = "mft:1:26".into();
    repository
        .upsert(&record, persistence_source::fjall::PersistMode::SyncAll)
        .unwrap();
    assert_ne!(
        native_catalog_snapshot(&first.conn, &a),
        native_catalog_snapshot(&second.conn, &b)
    );
}

#[test]
#[ignore = "explicit synthetic benchmark; use FORENSICS_IMPORT_BENCH_RECORDS"]
fn synthetic_import_profile() {
    let count = std::env::var("FORENSICS_IMPORT_BENCH_RECORDS")
        .unwrap_or_else(|_| "100000".into())
        .parse()
        .unwrap();
    let temporary = tempfile::TempDir::new().unwrap();
    let path = temporary.path().join("synthetic.raw");
    std::fs::write(&path, fixture::image(count)).unwrap();
    let result = import(&path, true);
    verify_report(&result.report, count as u64);
    emit_benchmark(&result);
}

#[test]
#[ignore = "requires explicit FORENSICS_IMPORT_FIXTURE RAW/E01 NTFS evidence"]
fn fixture_import_profile() {
    let path = std::env::var_os("FORENSICS_IMPORT_FIXTURE")
        .map(std::path::PathBuf::from)
        .expect("set FORENSICS_IMPORT_FIXTURE");
    let result = import(&path, true);
    assert!(!result.catalog.is_empty());
    emit_benchmark(&result);
}

#[test]
fn parallel_raw_and_e01_keep_the_serial_catalog_graph_and_timeline() {
    let temporary = tempfile::tempdir().unwrap();
    let raw = temporary.path().join("parallel.raw");
    let e01 = temporary.path().join("parallel.E01");
    let bytes = fixture::image(20_003);
    std::fs::write(&raw, &bytes).unwrap();
    fixture::write_e01(&e01, &bytes).unwrap();
    let serial = import_with_workers(&raw, true, 1);
    for path in [&raw, &e01] {
        let parallel = import_with_workers(path, true, 4);
        assert_eq!(serial.catalog, parallel.catalog);
        assert_eq!(serial.graph, parallel.graph);
        assert_eq!(serial.timeline, parallel.timeline);
        verify_report(&parallel.report, 20_003);
        let opens: u64 = parallel
            .report
            .partition_aggregates
            .iter()
            .filter(|sample| sample.phase == Phase::MftReaderOpen)
            .map(|sample| sample.full_image_opens)
            .sum();
        if app_services::parallel_enum::default_worker_count() > 1 {
            assert!(opens > 1);
        }
    }
}

#[test]
fn cancellation_before_enumeration_keeps_native_source_control_without_runtime_open() {
    let temporary = tempfile::tempdir().unwrap();
    let image = temporary.path().join("cancel.raw");
    std::fs::write(&image, fixture::image(96)).unwrap();
    let active =
        case_service::create_case(&temporary.path().join("cases"), "cancel", None).unwrap();
    active
        .with_conn(|conn| {
            let job = JobRepo::new(conn).create(&active.meta.id.0, "cancel")?;
            let config = import_precheck::prepare_import_source_config_from_path(
                &image.to_string_lossy(),
                DataSourcePlatform::Windows,
            )
            .unwrap();
            let cancel = Arc::new(AtomicBool::new(true));
            let options = ImportJobOptions {
                projection_policy: Default::default(),
                event_sink: None,
                cancel_token: &cancel,
                max_import_workers: Some(4),
                max_analysis_workers: Some(1),
                analysis_mode: ImportAnalysisMode::MetadataOnly,
                defer_linux_artifacts: false,
            };
            let profiler = ImportProfiler::default();
            assert!(execute_import_job_profiled(
                conn,
                &active.meta.id,
                &active.case_root,
                config,
                &job,
                options,
                &profiler
            )
            .is_err());
            let (source_id, state): (String, String) =
                conn.query_row("SELECT id, import_state FROM data_sources", [], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })?;
            assert_eq!(state, "failed");
            let source = source_db::open_registered_source_db_read_only(
                conn,
                &active.case_root,
                &DataSourceId(source_id),
            )?;
            let detail_tables: i64 = source.query_row(
                "SELECT COUNT(*) FROM sqlite_schema
             WHERE type = 'table'
               AND name IN ('file_entries', 'artifacts', 'timeline_events')",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(detail_tables, 0);
            let generation: String = source.query_row(
                "SELECT value FROM source_meta WHERE key = 'fjall_root'",
                [],
                |row| row.get(0),
            )?;
            assert!(generation == "source.fjall" || generation.starts_with("source.fjall.build."));
            assert!(!profiler
                .report()
                .samples
                .iter()
                .any(|sample| sample.phase == Phase::MftReaderOpen));
            Ok(())
        })
        .unwrap();
}

#[test]
#[ignore = "explicit benchmark matrix; requires FORENSICS_IMPORT_FIXTURE"]
fn matrix_import_profile() {
    let path = std::env::var_os("FORENSICS_IMPORT_FIXTURE")
        .map(std::path::PathBuf::from)
        .expect("set FORENSICS_IMPORT_FIXTURE");
    let workers: usize = std::env::var("FORENSICS_IMPORT_BENCH_WORKERS")
        .unwrap_or_else(|_| "1".into())
        .parse()
        .expect("valid worker count");
    assert!((1..=4).contains(&workers));
    eprintln!("[import-benchmark] {}", streaming::profile(&path, workers));
}

#[test]
#[ignore = "explicit synthetic RAW/E01 fixture preparation; requires FORENSICS_IMPORT_BENCH_DIR"]
fn prepare_import_benchmark_fixture() {
    let directory = std::env::var_os("FORENSICS_IMPORT_BENCH_DIR")
        .map(std::path::PathBuf::from)
        .expect("set FORENSICS_IMPORT_BENCH_DIR");
    let records: usize = std::env::var("FORENSICS_IMPORT_BENCH_RECORDS")
        .expect("set FORENSICS_IMPORT_BENCH_RECORDS")
        .parse()
        .expect("valid record count");
    std::fs::create_dir(&directory).expect("new fixture directory");
    let raw = directory.join("synthetic.raw");
    let e01 = directory.join("synthetic.E01");
    let scenario =
        std::env::var("FORENSICS_IMPORT_BENCH_SCENARIO").unwrap_or_else(|_| "resident".into());
    let bytes = scenarios::image(records, &scenario);
    std::fs::write(&raw, &bytes).unwrap();
    fixture::write_e01(&e01, &bytes).unwrap();
    eprintln!(
        "[fixture-prepared] records={records} logicalSha256={}",
        hex::encode(Sha256::digest(&bytes))
    );
}

#[test]
fn streaming_oracle_is_order_independent_and_detects_changed_rows() {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE items(id TEXT PRIMARY KEY,payload); INSERT INTO items VALUES('mft:0:20',42),('mft:0:1','hello')").unwrap();
    let normalizer = streaming::configure(&connection, vec![]);
    let first = streaming::fingerprint(&connection, &["SELECT id,payload FROM items"], &normalizer);
    connection
        .execute_batch(
            "DELETE FROM items; INSERT INTO items VALUES('mft:0:1','hello'),('mft:0:20',42)",
        )
        .unwrap();
    assert_eq!(
        first,
        streaming::fingerprint(&connection, &["SELECT id,payload FROM items"], &normalizer)
    );
    connection
        .execute_batch("UPDATE items SET payload='42' WHERE id='mft:0:20'")
        .unwrap();
    assert_ne!(
        first,
        streaming::fingerprint(&connection, &["SELECT id,payload FROM items"], &normalizer)
    );
}

#[test]
fn streaming_oracle_normalizes_generated_ids_in_rows_and_sort_keys() {
    fn fingerprint() -> streaming::Fingerprint {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE items(id TEXT PRIMARY KEY,payload)")
            .unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO items VALUES (?1,?2)",
            rusqlite::params![id, format!("parent:{id}")],
        )
        .unwrap();
        let normalizer = streaming::configure(&conn, vec![(id, "__root__".into())]);
        streaming::fingerprint(&conn, &["SELECT id,payload FROM items"], &normalizer)
    }
    assert_eq!(fingerprint(), fingerprint());
}
