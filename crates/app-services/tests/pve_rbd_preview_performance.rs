use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use app_services::{
    file_service::{
        close_preview_session_for_case, open_preview_session_for_case,
        read_preview_session_bytes_for_case, read_preview_session_media_range_for_case,
        FileServiceError, PreviewRuntimeRegistry, PreviewRuntimeStats,
    },
    import_analysis::{current_rss_mb, peak_rss_mb},
    source_db::{GlobalFileId, SourceConnectionManager},
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use domain::{CaseId, DataSourceKind, FileEntryId};
use persistence_sqlite::repositories::{case_repo::CaseRepo, datasource_repo::DataSourceRepo};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use transport::dto::MediaRangeRequestDto;

const CASE_ROOT_ENV: &str = "FORENSICS_PVE_RBD_PREVIEW_CASE_ROOT";
const ORACLE_ENV: &str = "FORENSICS_PVE_RBD_PREVIEW_ORACLE";
const DEFAULT_ORACLE_RELATIVE_PATH: &str =
    "../../testdata/real-samples/pve-rbd-preview-oracle.json";
const MAX_RUNTIME_CACHE_BYTES: usize = 128 * 1024 * 1024;
const MAX_RSS_DELTA_MB: i64 = 640;
const RANGE_64_KIB: u32 = 64 * 1024;
const RANGE_1_MIB: u32 = 1024 * 1024;
const LIFECYCLE_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
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
    filesystem_count: usize,
    session_count: usize,
    provider_constructions: u64,
    filesystem_constructions: u64,
    runtime_cache_capacity_bytes: usize,
    max_sessions: usize,
    max_runtimes: usize,
    max_filesystems: usize,
    post_close_session_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MediaParityReport {
    offset: u64,
    requested_bytes: u32,
    viewer_bytes: usize,
    media_bytes: u32,
    media_base64_chars: usize,
    viewer_sha256: String,
    media_sha256: String,
    exact_match: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LifecycleCheckpoint {
    phase: &'static str,
    runtime_count: usize,
    filesystem_count: usize,
    session_count: usize,
    provider_constructions: u64,
    filesystem_constructions: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InvalidationCycleReport {
    scope: &'static str,
    drained: bool,
    old_handle_rejected: bool,
    open_while_retired_rejected: bool,
    pre_invalidation_sha256: String,
    post_reactivation_sha256: String,
    fixed_oracle_match: bool,
    provider_constructions_before: u64,
    provider_constructions_after_rebuild: u64,
    post_invalidation_session_count: usize,
    post_rebuild_close_session_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewLifecycleReport {
    media_parity: MediaParityReport,
    source_invalidation: InvalidationCycleReport,
    case_invalidation: InvalidationCycleReport,
    checkpoints: Vec<LifecycleCheckpoint>,
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
    lifecycle: PreviewLifecycleReport,
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
    let fixed_small_sha256 = sha256_hex(&small_bytes);
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

    let media_parity = verify_media_range_parity(
        &read_context,
        &medium_handle,
        u64::from(RANGE_64_KIB),
        RANGE_64_KIB,
    );
    let live_stats = registry.stats().expect("preview runtime statistics");
    assert_eq!(live_stats.runtime_count, 1);
    assert_eq!(live_stats.provider_constructions, 1);
    assert_eq!(live_stats.filesystem_count, 3);
    assert_eq!(live_stats.filesystem_constructions, 3);
    assert!(
        live_stats.runtime_cache_capacity_bytes <= MAX_RUNTIME_CACHE_BYTES,
        "runtime cache capacity {} exceeds {}",
        live_stats.runtime_cache_capacity_bytes,
        MAX_RUNTIME_CACHE_BYTES
    );
    assert_eq!(live_stats.session_count, handles.len());

    for handle in &handles {
        assert!(close_preview_session_for_case(&registry, &case_id, handle)
            .expect("close preview session"));
    }
    let post_close_stats = registry.stats().expect("post-close runtime statistics");
    assert_eq!(post_close_stats.session_count, 0);
    assert_eq!(post_close_stats.filesystem_count, 3);
    assert_eq!(post_close_stats.filesystem_constructions, 3);
    let lifecycle = verify_preview_lifecycle(
        &read_context,
        &source.id,
        &targets[0],
        &fixed_small_sha256,
        media_parity,
        live_stats,
        post_close_stats,
    );
    let rss_after_mb = current_rss_mb();

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
        lifecycle,
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

fn verify_media_range_parity(
    context: &PreviewReadContext<'_>,
    handle: &str,
    offset: u64,
    length: u32,
) -> MediaParityReport {
    let viewer_bytes = read_range(
        context.registry,
        context.case_conn,
        context.case_root,
        context.case_id,
        handle,
        offset,
        length,
    );
    let media = read_preview_session_media_range_for_case(
        context.registry,
        context.case_conn,
        context.case_root,
        context.case_id,
        &MediaRangeRequestDto {
            handle_id: handle.to_string(),
            offset,
            length,
        },
    )
    .expect("read media bytes from the preview session");
    let media_bytes = STANDARD
        .decode(media.bytes_base64.as_bytes())
        .expect("decode preview media base64");

    assert_eq!(media.offset, offset);
    assert_eq!(media.bytes_read as usize, media_bytes.len());
    assert_eq!(media.bytes_read as usize, viewer_bytes.len());
    assert_eq!(
        media_bytes, viewer_bytes,
        "viewer and media paths returned different evidence bytes"
    );

    MediaParityReport {
        offset,
        requested_bytes: length,
        viewer_bytes: viewer_bytes.len(),
        media_bytes: media.bytes_read,
        media_base64_chars: media.bytes_base64.len(),
        viewer_sha256: sha256_hex(&viewer_bytes),
        media_sha256: sha256_hex(&media_bytes),
        exact_match: true,
    }
}

fn verify_preview_lifecycle(
    context: &PreviewReadContext<'_>,
    data_source_id: &domain::DataSourceId,
    target: &TargetFile,
    fixed_sha256: &str,
    media_parity: MediaParityReport,
    steady_live: PreviewRuntimeStats,
    steady_closed: PreviewRuntimeStats,
) -> PreviewLifecycleReport {
    let mut checkpoints = vec![
        lifecycle_checkpoint("steady-state-live", steady_live),
        lifecycle_checkpoint("steady-state-closed", steady_closed),
    ];
    let source_invalidation = verify_invalidation_cycle(
        context,
        data_source_id,
        target,
        fixed_sha256,
        InvalidationScope::Source,
        ProviderConstructionExpectation {
            before: 1,
            after_rebuild: 2,
            filesystem_before: 3,
            filesystem_after_rebuild: 4,
        },
        &mut checkpoints,
    );
    let case_invalidation = verify_invalidation_cycle(
        context,
        data_source_id,
        target,
        fixed_sha256,
        InvalidationScope::Case,
        ProviderConstructionExpectation {
            before: 2,
            after_rebuild: 3,
            filesystem_before: 4,
            filesystem_after_rebuild: 5,
        },
        &mut checkpoints,
    );

    PreviewLifecycleReport {
        media_parity,
        source_invalidation,
        case_invalidation,
        checkpoints,
    }
}

#[derive(Clone, Copy)]
enum InvalidationScope {
    Source,
    Case,
}

#[derive(Clone, Copy)]
struct ProviderConstructionExpectation {
    before: u64,
    after_rebuild: u64,
    filesystem_before: u64,
    filesystem_after_rebuild: u64,
}

impl InvalidationScope {
    fn label(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Case => "case",
        }
    }

    fn retire(
        self,
        registry: &PreviewRuntimeRegistry,
        case_id: &CaseId,
        data_source_id: &domain::DataSourceId,
    ) -> Result<bool, FileServiceError> {
        match self {
            Self::Source => registry.retire_source_and_drain(
                &case_id.0,
                &data_source_id.0,
                LIFECYCLE_DRAIN_TIMEOUT,
            ),
            Self::Case => registry.retire_case_and_drain(&case_id.0, LIFECYCLE_DRAIN_TIMEOUT),
        }
    }

    fn reactivate(
        self,
        registry: &PreviewRuntimeRegistry,
        case_id: &CaseId,
        data_source_id: &domain::DataSourceId,
    ) -> Result<(), FileServiceError> {
        match self {
            Self::Source => registry.reactivate_source(&case_id.0, &data_source_id.0),
            Self::Case => registry.reactivate_case(&case_id.0),
        }
    }
}

fn verify_invalidation_cycle(
    context: &PreviewReadContext<'_>,
    data_source_id: &domain::DataSourceId,
    target: &TargetFile,
    fixed_sha256: &str,
    scope: InvalidationScope,
    expected_constructions: ProviderConstructionExpectation,
    checkpoints: &mut Vec<LifecycleCheckpoint>,
) -> InvalidationCycleReport {
    let handle = open_target(
        context.registry,
        context.case_conn,
        context.case_root,
        context.case_id,
        target,
    );
    let before_bytes = read_range(
        context.registry,
        context.case_conn,
        context.case_root,
        context.case_id,
        &handle,
        0,
        RANGE_64_KIB,
    );
    let before_sha256 = sha256_hex(&before_bytes);
    assert_eq!(
        before_sha256,
        fixed_sha256,
        "{} pre-invalidation bytes changed from the fixed oracle",
        scope.label()
    );
    let before = context
        .registry
        .stats()
        .expect("pre-invalidation preview statistics");
    assert_eq!(
        before.provider_constructions,
        expected_constructions.before,
        "{} invalidation unexpectedly rebuilt the provider before retirement",
        scope.label()
    );
    assert_eq!(
        before.filesystem_constructions,
        expected_constructions.filesystem_before,
        "{} invalidation unexpectedly rebuilt a filesystem before retirement",
        scope.label()
    );
    assert_eq!(before.session_count, 1);
    checkpoints.push(lifecycle_checkpoint(
        match scope {
            InvalidationScope::Source => "source-before-invalidation",
            InvalidationScope::Case => "case-before-invalidation",
        },
        before,
    ));

    let drained = scope
        .retire(context.registry, context.case_id, data_source_id)
        .expect("retire and drain preview scope");
    assert!(drained, "{} preview scope did not drain", scope.label());
    let invalidated = context
        .registry
        .stats()
        .expect("post-invalidation preview statistics");
    assert_eq!(invalidated.runtime_count, 0);
    assert_eq!(invalidated.filesystem_count, 0);
    assert_eq!(invalidated.session_count, 0);
    assert_eq!(
        invalidated.provider_constructions,
        expected_constructions.before,
        "{} invalidation must not construct a provider",
        scope.label()
    );
    assert_eq!(
        invalidated.filesystem_constructions,
        expected_constructions.filesystem_before,
        "{} invalidation must not construct a filesystem",
        scope.label()
    );
    checkpoints.push(lifecycle_checkpoint(
        match scope {
            InvalidationScope::Source => "source-invalidated",
            InvalidationScope::Case => "case-invalidated",
        },
        invalidated,
    ));

    assert_invalidated_handle_rejected(context, &handle);
    assert_retired_scope_rejects_open(context, target);
    scope
        .reactivate(context.registry, context.case_id, data_source_id)
        .expect("reactivate preview scope");

    let rebuilt_handle = open_target(
        context.registry,
        context.case_conn,
        context.case_root,
        context.case_id,
        target,
    );
    let rebuilt_bytes = read_range(
        context.registry,
        context.case_conn,
        context.case_root,
        context.case_id,
        &rebuilt_handle,
        0,
        RANGE_64_KIB,
    );
    let rebuilt_sha256 = sha256_hex(&rebuilt_bytes);
    assert_eq!(
        rebuilt_sha256,
        fixed_sha256,
        "{} cold rebuild changed fixed evidence bytes",
        scope.label()
    );
    let rebuilt = context
        .registry
        .stats()
        .expect("post-reactivation preview statistics");
    assert_eq!(rebuilt.runtime_count, 1);
    assert_eq!(rebuilt.filesystem_count, 1);
    assert_eq!(rebuilt.session_count, 1);
    assert_eq!(
        rebuilt.provider_constructions,
        expected_constructions.after_rebuild,
        "{} reactivation must perform exactly one cold provider rebuild",
        scope.label()
    );
    assert_eq!(
        rebuilt.filesystem_constructions,
        expected_constructions.filesystem_after_rebuild,
        "{} reactivation must perform exactly one cold filesystem rebuild",
        scope.label()
    );
    checkpoints.push(lifecycle_checkpoint(
        match scope {
            InvalidationScope::Source => "source-reactivated-rebuilt",
            InvalidationScope::Case => "case-reactivated-rebuilt",
        },
        rebuilt,
    ));

    assert!(
        close_preview_session_for_case(context.registry, context.case_id, &rebuilt_handle)
            .expect("close rebuilt preview session")
    );
    let closed = context
        .registry
        .stats()
        .expect("post-rebuild close preview statistics");
    assert_eq!(closed.session_count, 0);
    assert_eq!(
        closed.provider_constructions, expected_constructions.after_rebuild,
        "closing a session must not rebuild its provider"
    );
    assert_eq!(
        closed.filesystem_constructions, expected_constructions.filesystem_after_rebuild,
        "closing a session must not rebuild its filesystem"
    );
    checkpoints.push(lifecycle_checkpoint(
        match scope {
            InvalidationScope::Source => "source-rebuild-closed",
            InvalidationScope::Case => "case-rebuild-closed",
        },
        closed,
    ));

    InvalidationCycleReport {
        scope: scope.label(),
        drained,
        old_handle_rejected: true,
        open_while_retired_rejected: true,
        pre_invalidation_sha256: before_sha256,
        post_reactivation_sha256: rebuilt_sha256,
        fixed_oracle_match: true,
        provider_constructions_before: before.provider_constructions,
        provider_constructions_after_rebuild: rebuilt.provider_constructions,
        post_invalidation_session_count: invalidated.session_count,
        post_rebuild_close_session_count: closed.session_count,
    }
}

fn assert_invalidated_handle_rejected(context: &PreviewReadContext<'_>, handle: &str) {
    let error = read_preview_session_bytes_for_case(
        context.registry,
        context.case_conn,
        context.case_root,
        context.case_id,
        handle,
        0,
        1,
    )
    .expect_err("invalidated preview handle must fail");
    assert!(
        matches!(error, FileServiceError::NotFound(_)),
        "invalidated preview handle must return not found"
    );
}

fn assert_retired_scope_rejects_open(context: &PreviewReadContext<'_>, target: &TargetFile) {
    let error = open_preview_session_for_case(
        context.registry,
        context.case_conn,
        context.case_root,
        context.case_id,
        &target.file_id,
    )
    .expect_err("retired preview scope must reject new sessions");
    assert!(
        matches!(error, FileServiceError::NotFound(_)),
        "retired preview scope must return not found"
    );
}

fn lifecycle_checkpoint(phase: &'static str, stats: PreviewRuntimeStats) -> LifecycleCheckpoint {
    LifecycleCheckpoint {
        phase,
        runtime_count: stats.runtime_count,
        filesystem_count: stats.filesystem_count,
        session_count: stats.session_count,
        provider_constructions: stats.provider_constructions,
        filesystem_constructions: stats.filesystem_constructions,
    }
}

fn validate_fixed_oracle(report: &PreviewPerformanceReport) {
    let oracle_json = required_oracle_json();
    let expected: FixedOracleManifest =
        serde_json::from_str(&oracle_json).expect("parse fixed preview byte oracle");
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

fn required_oracle_json() -> String {
    let path = std::env::var_os(ORACLE_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DEFAULT_ORACLE_RELATIVE_PATH)
        });
    std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "{ORACLE_ENV} must point to the PVE RBD preview oracle, or place it at {}: {error}",
            path.display()
        )
    })
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
        sha256: sha256_hex(bytes),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
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
        filesystem_count: stats.filesystem_count,
        session_count: stats.session_count,
        provider_constructions: stats.provider_constructions,
        filesystem_constructions: stats.filesystem_constructions,
        runtime_cache_capacity_bytes: stats.runtime_cache_capacity_bytes,
        max_sessions: stats.max_sessions,
        max_runtimes: stats.max_runtimes,
        max_filesystems: stats.max_filesystems,
        post_close_session_count,
    }
}
