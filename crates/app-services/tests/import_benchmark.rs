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
            max_import_workers: Some(1), max_analysis_workers: Some(1),
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
        let roots: Vec<String> = source.prepare("SELECT id FROM file_entries WHERE parent_id IS NULL ORDER BY partition_index")?
            .query_map([], |row| row.get(0))?.collect::<Result<_, _>>()?;
        let mut replacements = vec![(source_id, "__source__".into()), (active.meta.id.0.clone(), "__case__".into())];
        replacements.extend(roots.into_iter().enumerate().map(|(i, id)| (id, format!("__root_{i}__"))));
        let catalog = snapshot(&source, "SELECT id, parent_id, path, name, entry_type, size, ext, deleted, hidden, system, read_only, archive, unix_mode, encrypted, created_at, modified_at, accessed_at, changed_at, hash_sha256, partition_index FROM file_entries", &replacements);
        let mut graph = snapshot(&source, "SELECT id, node_type, label, summary, tags FROM graph_nodes", &replacements);
        graph.extend(snapshot(&source, "SELECT id, source_id, target_id, edge_type, confidence, provenance FROM graph_edges", &replacements));
        let timeline = snapshot(&source, "SELECT id, source_object_id, event_type, ts, title, description, parser_id, parser_version, confidence, source_attribution, attrs FROM timeline_events", &replacements);
        Ok(ImportResult { catalog, graph, timeline, report })
    }).unwrap()
}

fn snapshot(conn: &Connection, sql: &str, replacements: &[(String, String)]) -> Vec<Vec<Value>> {
    let mut statement = conn.prepare(sql).unwrap();
    let columns = statement.column_count();
    let mut rows: Vec<Vec<Value>> = statement
        .query_map([], |row| (0..columns).map(|index| row.get(index)).collect())
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for row in &mut rows {
        for value in row {
            if let Value::Text(text) = value {
                for (original, replacement) in replacements {
                    *text = text.replace(original, replacement);
                }
            }
        }
    }
    rows.sort_by_cached_key(|row| format!("{row:?}"));
    rows
}

fn verify_report(report: &ImportBenchmarkReportDto, record_count: u64) {
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
