#[path = "support/import_ntfs_fixture.rs"]
mod fixture;

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
    let temporary = tempfile::TempDir::new().unwrap();
    let active = case_service::create_case(
        &temporary.path().join("cases"),
        "synthetic-benchmark",
        Some("tester"),
    )
    .unwrap();
    active.with_conn(|conn| {
        let job = JobRepo::new(conn).create(&active.meta.id.0, "Synthetic benchmark")?;
        let config = import_precheck::prepare_import_source_config_from_path(&path.to_string_lossy(), DataSourcePlatform::Windows).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let options = ImportJobOptions { event_sink: None, cancel_token: &cancel,
            max_import_workers: Some(workers), max_analysis_workers: Some(1),
            analysis_mode: ImportAnalysisMode::MetadataOnly, defer_linux_artifacts: false };
        let profiler = ImportProfiler::default();
        let result = if profiling {
            execute_import_job_profiled(conn, &active.meta.id, &active.case_root, config, &job, options, &profiler)
        } else {
            execute_import_job_with_counts(conn, &active.meta.id, &active.case_root, config, &job, options)
        };
        result.unwrap_or_else(|error| panic!("synthetic import failed: {error}"));
        // Capture import RSS before the correctness oracle allocates snapshots.
        let report = profiler.report();
        let (source_id, state): (String, String) = conn.query_row("SELECT id, import_state FROM data_sources", [], |row| Ok((row.get(0)?, row.get(1)?)))?;
        assert_eq!(state, "ready");
        let source = source_db::open_registered_source_db_read_only(conn, &active.case_root, &DataSourceId(source_id.clone()))?;
        let replacements = canonical_replacements(&source, &active.meta.id.0, &source_id);
        let catalog = snapshot(&source, "SELECT id, parent_id, path, name, entry_type, size, ext, deleted, hidden, system, read_only, archive, unix_mode, encrypted, created_at, modified_at, accessed_at, changed_at, hash_sha256, partition_index FROM file_entries", &replacements);
        let mut graph = snapshot(&source, "SELECT id, node_type, label, summary, tags FROM graph_nodes", &replacements);
        graph.extend(snapshot(&source, "SELECT id, source_id, target_id, edge_type, confidence, provenance FROM graph_edges", &replacements));
        let timeline = snapshot(&source, "SELECT id, source_object_id, event_type, ts, title, description, parser_id, parser_version, confidence, source_attribution, attrs FROM timeline_events", &replacements);
        Ok(ImportResult { catalog, graph, timeline, report })
    }).unwrap()
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
    let generated: Vec<(String, Option<i64>, String)> = conn.prepare(
        "SELECT id, partition_index, path FROM file_entries WHERE parent_id IS NOT NULL AND length(id) = 36 AND id GLOB '[0-9a-fA-F]*'"
    ).unwrap().query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).unwrap().collect::<Result<_, _>>().unwrap();
    let mut identities = std::collections::HashSet::new();
    for (id, partition, path) in generated {
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
