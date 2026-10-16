#[path = "support/import_ntfs_fixture.rs"]
mod fixture;
#[path = "support/import_benchmark_queries.rs"]
mod queries;
#[path = "support/import_benchmark_stream.rs"]
mod streaming;

use app_services::{
    case_service,
    import_analysis::ImportAnalysisMode,
    import_pipeline::{
        execute_import_job_profiled, execute_import_job_with_counts, ImportJobOptions,
        ImportProfiler,
    },
    import_precheck, source_db,
};
use domain::{DataSourceId, DataSourcePlatform};
use persistence_sqlite::repositories::job_repo::JobRepo;
use rusqlite::{types::Value, Connection};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;
use std::sync::{atomic::AtomicBool, Arc};
use transport::dto::import_benchmark::{
    ImportBenchmarkPhaseDto as Phase, ImportBenchmarkReportDto,
};

struct ImportResult {
    catalog: Vec<Vec<Value>>,
    graph: Vec<Vec<Value>>,
    timeline: Vec<Vec<Value>>,
    report: ImportBenchmarkReportDto,
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
            let catalog = snapshot(source, queries::CATALOG, &replacements);
            let mut graph = snapshot(source, queries::NODES, &replacements);
            graph.extend(snapshot(source, queries::EDGES, &replacements));
            let timeline = snapshot(source, queries::TIMELINE, &replacements);
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
    let temporary = tempfile::TempDir::new().unwrap();
    let active = case_service::create_case(
        &temporary.path().join("cases"),
        "synthetic-benchmark",
        Some("tester"),
    )
    .unwrap();
    active
        .with_conn(|conn| {
            let job = JobRepo::new(conn).create(&active.meta.id.0, "Synthetic benchmark")?;
            let config = import_precheck::prepare_import_source_config_from_path(
                &path.to_string_lossy(),
                DataSourcePlatform::Windows,
            )
            .unwrap();
            let cancel = Arc::new(AtomicBool::new(false));
            let options = ImportJobOptions {
                event_sink: None,
                cancel_token: &cancel,
                max_import_workers: Some(workers),
                max_analysis_workers: Some(1),
                analysis_mode: ImportAnalysisMode::MetadataOnly,
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
            Ok(capture(&source, &active.meta.id.0, &source_id, report))
        })
        .unwrap()
}

fn canonical_replacements(
    conn: &Connection,
    case_id: &str,
    source_id: &str,
) -> Vec<(String, String)> {
    let roots: Vec<String> = conn
        .prepare("SELECT id FROM file_entries WHERE parent_id IS NULL ORDER BY partition_index")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
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
    let mut statement = conn.prepare(
        "SELECT id, partition_index, path FROM file_entries WHERE parent_id IS NOT NULL AND length(id) = 36 AND id GLOB '[0-9a-fA-F]*'"
    ).unwrap();
    let mut rows = statement.query([]).unwrap();
    let mut identities = std::collections::HashSet::new();
    let mut replacement_bytes = 0usize;
    while let Some(row) = rows.next().unwrap() {
        let id: String = row.get(0).unwrap();
        let partition: Option<i64> = row.get(1).unwrap();
        let path: String = row.get(2).unwrap();
        if uuid::Uuid::parse_str(&id).is_ok_and(|id| id.get_version_num() == 4) {
            let stable = format!(
                "__file_{}:{}:{}__",
                partition.unwrap_or(-1),
                path.len(),
                path
            );
            assert!(
                identities.insert(stable.clone()),
                "ambiguous generated-file identity in benchmark oracle"
            );
            replacement_bytes += id.len() + stable.len() + 128;
            assert!(
                replacements.len() < 100_000 && replacement_bytes <= 64 * 1024 * 1024,
                "benchmark UUID normalization map exceeds its 64 MiB / 100k-entry limit"
            );
            replacements.push((id, stable));
        }
    }
    replacements
}

fn snapshot(conn: &Connection, sql: &str, replacements: &[(String, String)]) -> Vec<Vec<Value>> {
    let mut statement = conn.prepare(sql).unwrap();
    let columns = statement.column_count();
    let mut rows: Vec<Vec<Value>> = statement
        .query_map([], |row| (0..columns).map(|index| row.get(index)).collect())
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
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
        Phase::Graph,
        Phase::Analysis,
        Phase::Timeline,
        Phase::TimelineEvents,
        Phase::TimelineGraph,
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
    fn catalog() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE file_entries (id TEXT PRIMARY KEY, parent_id TEXT, path TEXT, partition_index INTEGER)").unwrap();
        let root = uuid::Uuid::new_v4().to_string();
        let generated = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO file_entries VALUES (?1,NULL,'[P1]',1)",
            [&root],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO file_entries VALUES (?1,?2,'[P1]/note.txt',1)",
            [&generated, &root],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO file_entries VALUES ('mft:1:25',?1,'[P1]/native.txt',1)",
            [&root],
        )
        .unwrap();
        conn
    }
    let first = catalog();
    let second = catalog();
    let a = canonical_replacements(&first, "case", "source");
    let b = canonical_replacements(&second, "case", "source");
    let sql = "SELECT id,parent_id,path FROM file_entries";
    assert_eq!(snapshot(&first, sql, &a), snapshot(&second, sql, &b));
    let edges = "SELECT 'contains:' || parent_id || ':' || id, id FROM file_entries WHERE parent_id IS NOT NULL";
    assert_eq!(snapshot(&first, edges, &a), snapshot(&second, edges, &b));
    second
        .execute(
            "UPDATE file_entries SET id='mft:1:26' WHERE id='mft:1:25'",
            [],
        )
        .unwrap();
    assert_ne!(snapshot(&first, sql, &a), snapshot(&second, sql, &b));
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
fn cancellation_before_enumeration_keeps_source_indexes_and_skips_runtime_open() {
    let temporary = tempfile::tempdir().unwrap();
    let image = temporary.path().join("cancel.raw");
    std::fs::write(&image, fixture::image(96)).unwrap();
    let active =
        case_service::create_case(&temporary.path().join("cases"), "cancel", None).unwrap();
    active.with_conn(|conn| {
        let job = JobRepo::new(conn).create(&active.meta.id.0, "cancel")?;
        let config = import_precheck::prepare_import_source_config_from_path(
            &image.to_string_lossy(), DataSourcePlatform::Windows).unwrap();
        let cancel = Arc::new(AtomicBool::new(true));
        let options = ImportJobOptions { event_sink: None, cancel_token: &cancel,
            max_import_workers: Some(4), max_analysis_workers: Some(1),
            analysis_mode: ImportAnalysisMode::MetadataOnly, defer_linux_artifacts: false };
        let profiler = ImportProfiler::default();
        assert!(execute_import_job_profiled(conn, &active.meta.id, &active.case_root,
            config, &job, options, &profiler).is_err());
        let (source_id, state): (String, String) = conn.query_row(
            "SELECT id, import_state FROM data_sources", [], |row| Ok((row.get(0)?, row.get(1)?)))?;
        assert_eq!(state, "failed");
        let source = source_db::open_registered_source_db_read_only(conn, &active.case_root, &DataSourceId(source_id))?;
        let active_indexes: bool = source.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='idx_source_file_entries_path')", [], |row| row.get(0))?;
        assert!(active_indexes);
        assert!(!profiler.report().samples.iter().any(|sample| sample.phase == Phase::MftReaderOpen));
        Ok(())
    }).unwrap();
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
    let bytes = fixture::image(records);
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
