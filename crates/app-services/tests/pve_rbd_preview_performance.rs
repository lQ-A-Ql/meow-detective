use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use app_services::{
    file_service::{
        close_preview_session_for_case, open_preview_session_for_case,
        read_preview_session_bytes_for_case, PreviewRuntimeRegistry, PreviewRuntimeStats,
    },
    import_analysis::{current_rss_mb, peak_rss_mb},
    source_db::{GlobalFileId, SourceConnectionManager},
};
use domain::{CaseId, DataSourceKind, FileEntryId};
use persistence_sqlite::repositories::{case_repo::CaseRepo, datasource_repo::DataSourceRepo};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const CASE_ROOT_ENV: &str = "FORENSICS_PVE_RBD_PREVIEW_CASE_ROOT";
const MAX_RUNTIME_CACHE_BYTES: usize = 128 * 1024 * 1024;
const MAX_RSS_DELTA_MB: i64 = 640;
const RANGE_64_KIB: u32 = 64 * 1024;
const RANGE_1_MIB: u32 = 1024 * 1024;
const FIXED_ORACLE_JSON: &str =
    include_str!("../../../testdata/real-samples/pve-rbd-preview-oracle.json");

const ROOT_DIRECT: &str = "Partition 0 (XFS) - Partition 0";
const ROOT_HOME: &str = "Partition 1 (XFS) - centos/home";
const ROOT_SYSTEM: &str = "Partition 2 (XFS) - centos/root";

const SMALL_PATH: &str = "etc/passwd";
const DIRECT_PATH: &str = "grub2/locale/en@hebrew.mo";
const HOME_PATH: &str = "jinqin_backup.sql.gz.enc";
const MEDIUM_PATH: &str =
    "tmp/licai/phpvibe-video/.git/objects/pack/pack-a0fa91d356ffff961a5e571bad847deca6c9c986.pack";
const LARGE_PATH: &str = "var/www/html/licai.tar";

#[derive(Debug, Clone)]
struct TargetFile {
    label: &'static str,
    file_id: String,
    path: &'static str,
    size: u64,
}

struct PreviewReadContext<'a> {
    registry: &'a PreviewRuntimeRegistry,
    case_conn: &'a rusqlite::Connection,
    case_root: &'a Path,
    case_id: &'a CaseId,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileSummary {
    label: &'static str,
    path: &'static str,
    size: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RangeOracle {
    scenario: String,
    offset: u64,
    requested_bytes: u32,
    actual_bytes: usize,
    elapsed_ms: f64,
    sha256: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TimingMetric {
    scenario: &'static str,
    samples: usize,
    p50_ms: f64,
    p95_ms: f64,
    max_ms: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeStatsReport {
    runtime_count: usize,
    session_count: usize,
    provider_constructions: u64,
    runtime_cache_capacity_bytes: usize,
    max_sessions: usize,
    max_runtimes: usize,
    post_close_session_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MemoryReport {
    rss_before_mb: u64,
    rss_after_mb: u64,
    rss_delta_mb: i64,
    peak_rss_mb: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewPerformanceReport {
    schema_version: u32,
    case_id: String,
    data_source_id: String,
    files: Vec<FileSummary>,
    metrics: Vec<TimingMetric>,
    ranges: Vec<RangeOracle>,
    runtime: RuntimeStatsReport,
    memory: MemoryReport,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixedOracleManifest {
    schema_version: u32,
    files: Vec<FixedFileOracle>,
    ranges: Vec<FixedRangeOracle>,
}

#[derive(Debug, Deserialize)]
struct FixedFileOracle {
    label: String,
    path: String,
    size: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixedRangeOracle {
    scenario: String,
    offset: u64,
    requested_bytes: u32,
    actual_bytes: usize,
    sha256: String,
}

#[test]
#[ignore = "requires a retained PVE case with a ready derived RBD source"]
fn retained_pve_rbd_preview_performance() {
    let case_root = required_case_root();
    let case_conn = persistence_sqlite::connection::open_existing(&case_root.join("app.db"))
        .expect("open retained PVE case database");
    let case_id = only_case_id(&case_conn);
    let source = only_ready_rbd_source(&case_conn, &case_id);
    let source_conn = SourceConnectionManager::new(&case_root)
        .open_ready(&case_conn, &case_id, &source.id)
        .expect("open ready derived RBD source database");

    let targets = [
        find_target(&source_conn, &source.id, "small", ROOT_SYSTEM, SMALL_PATH),
        find_target(
            &source_conn,
            &source.id,
            "direct-xfs",
            ROOT_DIRECT,
            DIRECT_PATH,
        ),
        find_target(&source_conn, &source.id, "home-xfs", ROOT_HOME, HOME_PATH),
        find_target(
            &source_conn,
            &source.id,
            "medium-root-xfs",
            ROOT_SYSTEM,
            MEDIUM_PATH,
        ),
        find_target(
            &source_conn,
            &source.id,
            "large-root-xfs",
            ROOT_SYSTEM,
            LARGE_PATH,
        ),
    ];

    let registry = PreviewRuntimeRegistry::default();
    let read_context = PreviewReadContext {
        registry: &registry,
        case_conn: &case_conn,
        case_root: &case_root,
        case_id: &case_id,
    };
    let rss_before_mb = current_rss_mb();
    let mut handles = Vec::with_capacity(targets.len());
    let mut ranges = Vec::new();
    let mut metrics = Vec::new();

    let cold_started = Instant::now();
    let small_open_started = Instant::now();
    let small_handle = open_target(&registry, &case_conn, &case_root, &case_id, &targets[0]);
    let small_open_elapsed = small_open_started.elapsed();
    handles.push(small_handle.clone());
    let small_read_started = Instant::now();
    let small_bytes = read_range(
        &registry,
        &case_conn,
        &case_root,
        &case_id,
        &small_handle,
        0,
        RANGE_64_KIB,
    );
    let small_read_elapsed = small_read_started.elapsed();
    let cold_elapsed = cold_started.elapsed();
    ranges.push(oracle(
        "cold-small",
        0,
        RANGE_64_KIB,
        cold_elapsed,
        &small_bytes,
    ));
    metrics.push(metric("coldSmallOpen", &[small_open_elapsed]));
    metrics.push(metric("coldSmallRead", &[small_read_elapsed]));
    metrics.push(metric("coldSmallOpenRead", &[cold_elapsed]));

    for (target, open_scenario, read_scenario) in [
        (&targets[1], "directXfsOpen", "directXfs64KiB"),
        (&targets[2], "homeXfsOpen", "homeXfs64KiB"),
    ] {
        let open_started = Instant::now();
        let handle = open_target(&registry, &case_conn, &case_root, &case_id, target);
        let open_elapsed = open_started.elapsed();
        handles.push(handle.clone());
        let started = Instant::now();
        let bytes = read_range(
            &registry,
            &case_conn,
            &case_root,
            &case_id,
            &handle,
            0,
            RANGE_64_KIB,
        );
        let elapsed = started.elapsed();
        ranges.push(oracle(read_scenario, 0, RANGE_64_KIB, elapsed, &bytes));
        metrics.push(metric(open_scenario, &[open_elapsed]));
        metrics.push(metric(read_scenario, &[elapsed]));
    }

    let medium_open_started = Instant::now();
    let medium_handle = open_target(&registry, &case_conn, &case_root, &case_id, &targets[3]);
    let medium_open_elapsed = medium_open_started.elapsed();
    handles.push(medium_handle.clone());
    metrics.push(metric("mediumRootXfsOpen", &[medium_open_elapsed]));
    let warmup = timed_range(
        &read_context,
        &medium_handle,
        "medium-warmup",
        0,
        RANGE_64_KIB,
    );
    ranges.push(warmup.1);

    let mut repeated = Vec::with_capacity(12);
    for iteration in 0..12 {
        let (elapsed, range) = timed_range(
            &read_context,
            &medium_handle,
            &format!("warm-repeat-{iteration:02}"),
            0,
            RANGE_64_KIB,
        );
        repeated.push(elapsed);
        ranges.push(range);
    }
    assert_same_digest(&ranges[ranges.len() - repeated.len() - 1..]);
    metrics.push(metric("warmSame64KiB", &repeated));

    let mut sequential_64k = Vec::with_capacity(16);
    for index in 0..16u64 {
        let offset = index * u64::from(RANGE_64_KIB);
        let (elapsed, range) = timed_range(
            &read_context,
            &medium_handle,
            &format!("sequential-64k-{index:02}"),
            offset,
            RANGE_64_KIB,
        );
        sequential_64k.push(elapsed);
        ranges.push(range);
    }
    metrics.push(metric("sequential16x64KiB", &sequential_64k));

    let mut sequential_1m = Vec::with_capacity(4);
    for index in 0..4u64 {
        let offset = index * u64::from(RANGE_1_MIB);
        let (elapsed, range) = timed_range(
            &read_context,
            &medium_handle,
            &format!("sequential-1m-{index:02}"),
            offset,
            RANGE_1_MIB,
        );
        sequential_1m.push(elapsed);
        ranges.push(range);
    }
    metrics.push(metric("sequential4x1MiB", &sequential_1m));

    let large_open_started = Instant::now();
    let large_handle = open_target(&registry, &case_conn, &case_root, &case_id, &targets[4]);
    let large_open_elapsed = large_open_started.elapsed();
    handles.push(large_handle.clone());
    metrics.push(metric("largeRootXfsOpen", &[large_open_elapsed]));
    let large_warmup = timed_range(
        &read_context,
        &large_handle,
        "large-warmup",
        0,
        RANGE_64_KIB,
    );
    metrics.push(metric("largeRootXfsWarmup", &[large_warmup.0]));
    ranges.push(large_warmup.1);
    let large_size = targets[4].size;
    let random_offsets = [
        align_64k(large_size / 7),
        align_64k(large_size / 3),
        align_64k(large_size / 2),
        align_64k(large_size.saturating_mul(5) / 7),
        large_size.saturating_sub(u64::from(RANGE_64_KIB)),
    ];
    let mut random_large = Vec::with_capacity(random_offsets.len());
    for (index, offset) in random_offsets.into_iter().enumerate() {
        let (elapsed, range) = timed_range(
            &read_context,
            &large_handle,
            &format!("large-random-{index:02}"),
            offset,
            RANGE_64_KIB,
        );
        random_large.push(elapsed);
        ranges.push(range);
    }
    metrics.push(metric("largeRandom64KiB", &random_large));

    let live_stats = registry.stats().expect("preview runtime statistics");
    assert_eq!(live_stats.runtime_count, 1);
    assert_eq!(live_stats.provider_constructions, 1);
    assert!(
        live_stats.runtime_cache_capacity_bytes <= MAX_RUNTIME_CACHE_BYTES,
        "runtime cache capacity {} exceeds {}",
        live_stats.runtime_cache_capacity_bytes,
        MAX_RUNTIME_CACHE_BYTES
    );
    assert_eq!(live_stats.session_count, handles.len());

    let rss_after_mb = current_rss_mb();
    for handle in &handles {
        assert!(close_preview_session_for_case(&registry, &case_id, handle)
            .expect("close preview session"));
    }
    let post_close_stats = registry.stats().expect("post-close runtime statistics");
    assert_eq!(post_close_stats.session_count, 0);

    let report = PreviewPerformanceReport {
        schema_version: 1,
        case_id: case_id.0,
        data_source_id: source.id.0,
        files: targets
            .iter()
            .map(|target| FileSummary {
                label: target.label,
                path: target.path,
                size: target.size,
            })
            .collect(),
        metrics,
        ranges,
        runtime: runtime_report(live_stats, post_close_stats.session_count),
        memory: MemoryReport {
            rss_before_mb,
            rss_after_mb,
            rss_delta_mb: rss_after_mb as i64 - rss_before_mb as i64,
            peak_rss_mb: peak_rss_mb(),
        },
    };
    assert!(
        report.memory.rss_delta_mb <= MAX_RSS_DELTA_MB,
        "RSS delta {}MB exceeds {}MB",
        report.memory.rss_delta_mb,
        MAX_RSS_DELTA_MB
    );
    validate_fixed_oracle(&report);
    eprintln!(
        "PVE_RBD_PREVIEW_METRICS {}",
        serde_json::to_string(&report).expect("serialize preview performance report")
    );
}

fn validate_fixed_oracle(report: &PreviewPerformanceReport) {
    let expected: FixedOracleManifest =
        serde_json::from_str(FIXED_ORACLE_JSON).expect("parse fixed preview byte oracle");
    assert_eq!(expected.schema_version, 1, "fixed oracle schema version");
    for file in expected.files {
        assert!(
            report.files.iter().any(|candidate| {
                candidate.label == file.label
                    && candidate.path == file.path
                    && candidate.size == file.size
            }),
            "fixed file oracle changed for '{}'",
            file.label
        );
    }
    for range in expected.ranges {
        let actual = report
            .ranges
            .iter()
            .find(|candidate| {
                candidate.scenario == range.scenario
                    && candidate.offset == range.offset
                    && candidate.requested_bytes == range.requested_bytes
            })
            .unwrap_or_else(|| panic!("missing fixed range oracle '{}'", range.scenario));
        assert_eq!(
            actual.actual_bytes, range.actual_bytes,
            "fixed range length changed for '{}'",
            range.scenario
        );
        assert_eq!(
            actual.sha256, range.sha256,
            "fixed evidence bytes changed for '{}'",
            range.scenario
        );
    }
}

fn required_case_root() -> PathBuf {
    let case_root = std::env::var_os(CASE_ROOT_ENV)
        .map(PathBuf::from)
        .expect("FORENSICS_PVE_RBD_PREVIEW_CASE_ROOT must point to a retained case root");
    assert!(
        case_root.join("app.db").is_file(),
        "{CASE_ROOT_ENV} must contain app.db"
    );
    case_root
}

fn only_case_id(case_conn: &rusqlite::Connection) -> CaseId {
    let cases = CaseRepo::new(case_conn)
        .list_all()
        .expect("query retained cases");
    assert_eq!(cases.len(), 1, "retained preview case count");
    cases[0].id.clone()
}

fn only_ready_rbd_source(case_conn: &rusqlite::Connection, case_id: &CaseId) -> domain::DataSource {
    let repo = DataSourceRepo::new(case_conn);
    let sources = repo
        .find_by_case(case_id)
        .expect("query retained data sources")
        .into_iter()
        .filter(|source| source.kind == DataSourceKind::CephRbd)
        .filter(|source| {
            repo.find_storage(&source.id)
                .expect("query derived RBD storage")
                .is_some_and(|storage| storage.import_state == "ready")
        })
        .collect::<Vec<_>>();
    assert_eq!(sources.len(), 1, "ready derived RBD source count");
    sources
        .into_iter()
        .next()
        .expect("ready derived RBD source")
}

fn find_target(
    source_conn: &rusqlite::Connection,
    data_source_id: &domain::DataSourceId,
    label: &'static str,
    root_name: &str,
    path: &'static str,
) -> TargetFile {
    let (local_id, size) = source_conn
        .query_row(
            "WITH RECURSIVE tree(id) AS (
                 SELECT id
                 FROM file_entries
                 WHERE parent_id IS NULL AND data_source_id = ?1 AND name = ?2
                 UNION ALL
                 SELECT child.id
                 FROM file_entries child
                 JOIN tree parent ON child.parent_id = parent.id
             )
             SELECT entry.id, entry.size
             FROM tree
             JOIN file_entries entry ON entry.id = tree.id
             WHERE entry.entry_type = 'file' AND entry.path = ?3
             LIMIT 1",
            rusqlite::params![data_source_id.0, root_name, path],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)),
        )
        .unwrap_or_else(|error| panic!("find {label} target '{path}': {error}"));
    assert!(size > 0, "{label} target must not be empty");
    let file_id = GlobalFileId::new(data_source_id.clone(), FileEntryId(local_id))
        .encode()
        .0;
    TargetFile {
        label,
        file_id,
        path,
        size,
    }
}

fn open_target(
    registry: &PreviewRuntimeRegistry,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    target: &TargetFile,
) -> String {
    let handle =
        open_preview_session_for_case(registry, case_conn, case_root, case_id, &target.file_id)
            .unwrap_or_else(|error| {
                panic!(
                    "open preview target '{}' ({}): {error}",
                    target.label, target.path
                )
            });
    assert_eq!(handle.size, target.size);
    assert!(handle.handle_id.starts_with("preview:"));
    assert!(!handle.handle_id.contains(&target.file_id));
    handle.handle_id
}

fn timed_range(
    context: &PreviewReadContext<'_>,
    handle: &str,
    scenario: &str,
    offset: u64,
    length: u32,
) -> (Duration, RangeOracle) {
    let started = Instant::now();
    let bytes = read_range(
        context.registry,
        context.case_conn,
        context.case_root,
        context.case_id,
        handle,
        offset,
        length,
    );
    let elapsed = started.elapsed();
    (elapsed, oracle(scenario, offset, length, elapsed, &bytes))
}

fn read_range(
    registry: &PreviewRuntimeRegistry,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    handle: &str,
    offset: u64,
    length: u32,
) -> Vec<u8> {
    let bytes = read_preview_session_bytes_for_case(
        registry, case_conn, case_root, case_id, handle, offset, length,
    )
    .unwrap_or_else(|error| panic!("read preview range offset={offset} length={length}: {error}"));
    assert!(
        !bytes.is_empty(),
        "preview range must return evidence bytes"
    );
    assert!(bytes.len() <= length as usize);
    bytes
}

fn oracle(
    scenario: impl Into<String>,
    offset: u64,
    requested_bytes: u32,
    elapsed: Duration,
    bytes: &[u8],
) -> RangeOracle {
    RangeOracle {
        scenario: scenario.into(),
        offset,
        requested_bytes,
        actual_bytes: bytes.len(),
        elapsed_ms: duration_ms(elapsed),
        sha256: hex::encode(Sha256::digest(bytes)),
    }
}

fn metric(scenario: &'static str, samples: &[Duration]) -> TimingMetric {
    assert!(!samples.is_empty());
    let mut values = samples.iter().copied().map(duration_ms).collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    TimingMetric {
        scenario,
        samples: values.len(),
        p50_ms: percentile(&values, 0.50),
        p95_ms: percentile(&values, 0.95),
        max_ms: *values.last().expect("timing sample"),
    }
}

fn percentile(sorted: &[f64], quantile: f64) -> f64 {
    let rank = ((sorted.len() as f64 * quantile).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1);
    sorted[rank]
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn align_64k(offset: u64) -> u64 {
    offset / u64::from(RANGE_64_KIB) * u64::from(RANGE_64_KIB)
}

fn assert_same_digest(ranges: &[RangeOracle]) {
    let expected = &ranges[0].sha256;
    assert!(
        ranges.iter().all(|range| &range.sha256 == expected),
        "repeated warm reads returned different evidence bytes"
    );
}

fn runtime_report(
    stats: PreviewRuntimeStats,
    post_close_session_count: usize,
) -> RuntimeStatsReport {
    RuntimeStatsReport {
        runtime_count: stats.runtime_count,
        session_count: stats.session_count,
        provider_constructions: stats.provider_constructions,
        runtime_cache_capacity_bytes: stats.runtime_cache_capacity_bytes,
        max_sessions: stats.max_sessions,
        max_runtimes: stats.max_runtimes,
        post_close_session_count,
    }
}
