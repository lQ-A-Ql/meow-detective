use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use app_services::{
    datasource_service::{
        detect_image_filesystem, expand_lvm_pool_candidates, ImageFilesystemCandidate,
        ImageFilesystemKind, ImageFilesystemSource,
    },
    import_analysis::{current_rss_mb, peak_rss_mb},
};
use domain::DataSourceKind;
use evidence_core::{EvidenceReader, FileSystemReader};
use fs_ext4::Ext4Reader;
use image_e01::E01Reader;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const CLUSTER_ROOT_ENV: &str = "FORENSICS_PVE_CLUSTER_ROOT";
const PREFERRED_MEMBER_NAME: &str = "server01-disk01.e01";
const RANGE_64_KIB: usize = 64 * 1024;
const RANGE_1_MIB: usize = 1024 * 1024;
const SEQUENTIAL_64_KIB_COUNT: usize = 16;
const SEQUENTIAL_1_MIB_COUNT: usize = 4;
const WARM_REPEAT_COUNT: usize = 12;
const MAX_DIRECTORY_DEPTH: usize = 64;
const MAX_EXT4_JOURNAL_BYTES: usize = 128 * 1024 * 1024;
const MAX_EXT4_RECOVERY_SECONDS: u64 = 180;
const FINGERPRINT_SAMPLE_BYTES: usize = 64 * 1024;
const FIXED_ORACLE_JSON: &str =
    include_str!("../../../testdata/real-samples/pve-host-ext4-preview-oracle.json");

#[derive(Debug, Clone)]
struct TargetFile {
    logical_path: String,
    size: u64,
    selection_tier: &'static str,
}

#[derive(Default)]
struct TargetFallbacks {
    at_least_64_kib: Option<TargetFile>,
    at_least_1_mib: Option<TargetFile>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TimingMetric {
    scenario: &'static str,
    status: &'static str,
    samples: usize,
    bytes_per_sample: usize,
    p50_ms: Option<f64>,
    p95_ms: Option<f64>,
    max_ms: Option<f64>,
    digest_sha256: Option<String>,
    skipped_reason: Option<&'static str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MemoryMetric {
    rss_before_mb: u64,
    rss_after_mb: u64,
    rss_delta_mb: i64,
    peak_rss_mb: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HostPreviewReport {
    schema_version: u32,
    member_fingerprint: String,
    filesystem: &'static str,
    logical_volume: &'static str,
    logical_file_path: String,
    file_size: u64,
    selection_tier: &'static str,
    oracle_verified: bool,
    metrics: Vec<TimingMetric>,
    memory: MemoryMetric,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixedOracle {
    schema_version: u32,
    member_fingerprint: String,
    filesystem: String,
    logical_volume: String,
    logical_file_path: String,
    file_size: u64,
    metrics: Vec<FixedMetric>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixedMetric {
    scenario: String,
    status: String,
    samples: usize,
    bytes_per_sample: usize,
    digest_sha256: String,
}

#[test]
#[ignore = "requires FORENSICS_PVE_CLUSTER_ROOT with the private PVE cluster E01 sample"]
fn pve_host_ext4_native_preview_performance() {
    let cluster_root = required_cluster_root();
    let member = select_host_member(&cluster_root);
    let member_fingerprint = fingerprint_member(&member);

    let discovery_candidate = discover_pve_root_ext4(&member);
    let discovery_fs = open_pve_root_ext4(&member, &discovery_candidate);
    let target = select_target_file(&discovery_fs);
    drop(discovery_fs);

    assert!(
        target.size >= RANGE_64_KIB as u64,
        "selected ordinary file must be at least 64 KiB"
    );
    assert_sanitized_logical_path(&target.logical_path);

    let rss_before_mb = current_rss_mb();
    let cold_open_read_started = Instant::now();
    let cold_open_started = Instant::now();
    let fs = open_pve_root_ext4(&member, &discovery_candidate);
    let cold_open_elapsed = cold_open_started.elapsed();
    let cold_read_started = Instant::now();
    let cold_bytes = read_exact_range(&fs, &target, 0, RANGE_64_KIB);
    let cold_read_elapsed = cold_read_started.elapsed();
    let cold_open_read_elapsed = cold_open_read_started.elapsed();

    let mut metrics = vec![
        measured_metric("coldOpen", RANGE_64_KIB, &[cold_open_elapsed], None),
        measured_metric(
            "coldRead64KiB",
            RANGE_64_KIB,
            &[cold_read_elapsed],
            Some(digest(&cold_bytes)),
        ),
        measured_metric(
            "coldOpenRead64KiB",
            RANGE_64_KIB,
            &[cold_open_read_elapsed],
            Some(digest(&cold_bytes)),
        ),
    ];

    let (warm_timings, warm_digest) =
        measure_repeated_range(&fs, &target, 0, RANGE_64_KIB, WARM_REPEAT_COUNT);
    assert_eq!(
        warm_digest,
        digest(&cold_bytes),
        "cold and warm reads must return identical evidence bytes"
    );
    metrics.push(measured_metric(
        "warmSame64KiB",
        RANGE_64_KIB,
        &warm_timings,
        Some(warm_digest),
    ));

    metrics.push(measure_sequential_or_skip(
        &fs,
        &target,
        "sequential16x64KiB",
        RANGE_64_KIB,
        SEQUENTIAL_64_KIB_COUNT,
        "file_size_below_1_mib",
    ));
    metrics.push(measure_sequential_or_skip(
        &fs,
        &target,
        "sequential4x1MiB",
        RANGE_1_MIB,
        SEQUENTIAL_1_MIB_COUNT,
        "file_size_below_4_mib",
    ));

    let rss_after_mb = current_rss_mb();
    let report = HostPreviewReport {
        schema_version: 1,
        member_fingerprint,
        filesystem: "ext4",
        logical_volume: "pve/root",
        logical_file_path: target.logical_path,
        file_size: target.size,
        selection_tier: target.selection_tier,
        oracle_verified: true,
        metrics,
        memory: MemoryMetric {
            rss_before_mb,
            rss_after_mb,
            rss_delta_mb: rss_after_mb as i64 - rss_before_mb as i64,
            peak_rss_mb: peak_rss_mb(),
        },
    };
    validate_fixed_oracle(&report);

    println!(
        "PVE_HOST_PREVIEW_METRICS {}",
        serde_json::to_string(&report).expect("serialize PVE host preview metrics")
    );
}

#[test]
#[ignore = "requires FORENSICS_PVE_CLUSTER_ROOT with the private PVE cluster E01 sample"]
fn pve_host_ext4_deleted_recovery_is_bounded_and_proven() {
    let started = Instant::now();
    let cluster_root = required_cluster_root();
    let member = select_host_member(&cluster_root);
    let candidate = discover_pve_root_ext4(&member);
    let filesystem = open_pve_root_ext4(&member, &candidate);
    let journal = filesystem
        .read_internal_journal(MAX_EXT4_JOURNAL_BYTES)
        .expect("pve/root internal ext4 journal should fit the bounded recovery snapshot");
    assert!(!journal.is_empty());
    assert!(journal.len() <= MAX_EXT4_JOURNAL_BYTES);

    let candidates = fs_ext4::journal::recover_deleted_inodes(&filesystem, &journal)
        .expect("pve/root JBD2 journal should remain parseable");
    for candidate in &candidates {
        for range in &candidate.content_mapping.ranges {
            if range.kind == fs_ext4::journal::DeletedContentRangeKind::RecoverableData {
                assert_eq!(
                    range.allocation_state,
                    fs_ext4::journal::RecoveryAllocationState::Free
                );
                assert!(range.filesystem_source_offset.is_some());
                assert!(range.sha256.is_some());
            }
        }
        match candidate.completeness {
            fs_ext4::journal::RecoveryCompleteness::Complete => {
                assert_eq!(
                    candidate.content_mapping.inode_allocation_state,
                    fs_ext4::journal::RecoveryAllocationState::Free
                );
                assert_eq!(candidate.recoverable_bytes, candidate.declared_size);
                assert!(candidate.content_mapping.content_sha256.is_some());
                assert_eq!(
                    candidate.content_mapping.data_allocation_state,
                    fs_ext4::journal::RecoveryAllocationState::Free
                );
            }
            fs_ext4::journal::RecoveryCompleteness::Partial => {
                assert!(candidate.recoverable_bytes > 0);
                assert!(candidate.recoverable_bytes < candidate.declared_size);
            }
            fs_ext4::journal::RecoveryCompleteness::MetadataOnly => {
                assert_eq!(candidate.recoverable_bytes, 0);
            }
        }
    }
    let elapsed = started.elapsed();
    eprintln!(
        "PVE ext4 deleted recovery: member={} journal_bytes={} candidates={} elapsed_ms={} peak_rss_mb={}",
        member
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown"),
        journal.len(),
        candidates.len(),
        elapsed.as_millis(),
        peak_rss_mb()
    );
    assert!(
        elapsed <= Duration::from_secs(MAX_EXT4_RECOVERY_SECONDS),
        "bounded PVE ext4 journal recovery exceeded {} seconds: {:?}",
        MAX_EXT4_RECOVERY_SECONDS,
        elapsed
    );
}

fn validate_fixed_oracle(report: &HostPreviewReport) {
    let expected: FixedOracle =
        serde_json::from_str(FIXED_ORACLE_JSON).expect("parse PVE host EXT4 preview oracle");
    assert_eq!(expected.schema_version, report.schema_version);
    assert_eq!(expected.member_fingerprint, report.member_fingerprint);
    assert_eq!(expected.filesystem, report.filesystem);
    assert_eq!(expected.logical_volume, report.logical_volume);
    assert_eq!(expected.logical_file_path, report.logical_file_path);
    assert_eq!(expected.file_size, report.file_size);
    for expected_metric in expected.metrics {
        let actual = report
            .metrics
            .iter()
            .find(|candidate| candidate.scenario == expected_metric.scenario)
            .unwrap_or_else(|| {
                panic!(
                    "missing fixed PVE host metric '{}'",
                    expected_metric.scenario
                )
            });
        assert_eq!(actual.status, expected_metric.status);
        assert_eq!(actual.samples, expected_metric.samples);
        assert_eq!(actual.bytes_per_sample, expected_metric.bytes_per_sample);
        assert_eq!(
            actual.digest_sha256.as_deref(),
            Some(expected_metric.digest_sha256.as_str())
        );
    }
}

fn required_cluster_root() -> PathBuf {
    let root = std::env::var_os(CLUSTER_ROOT_ENV)
        .map(PathBuf::from)
        .expect("FORENSICS_PVE_CLUSTER_ROOT must point to the PVE cluster sample directory");
    assert!(
        root.is_dir(),
        "FORENSICS_PVE_CLUSTER_ROOT must identify a readable directory"
    );
    root
}

fn select_host_member(root: &Path) -> PathBuf {
    let mut members = collect_e01_members(root)
        .into_iter()
        .filter(|path| {
            path.file_stem()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.to_ascii_lowercase().ends_with("-disk01"))
        })
        .collect::<Vec<_>>();
    members.sort_by_key(|path| stable_path_key(path));

    members
        .iter()
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case(PREFERRED_MEMBER_NAME))
        })
        .cloned()
        .or_else(|| members.into_iter().next())
        .expect("PVE cluster sample must contain at least one disk01 E01 member")
}

fn collect_e01_members(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let mut entries = std::fs::read_dir(&directory)
            .expect("read PVE cluster sample directory")
            .collect::<Result<Vec<_>, _>>()
            .expect("enumerate PVE cluster sample directory");
        entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase());
        for entry in entries.into_iter().rev() {
            let path = entry.path();
            let file_type = entry.file_type().expect("read PVE sample member type");
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("e01"))
            {
                files.push(path);
            }
        }
    }
    files
}

fn stable_path_key(path: &Path) -> String {
    path.components()
        .rev()
        .take(2)
        .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("/")
}

fn fingerprint_member(path: &Path) -> String {
    let mut file = File::open(path).expect("open PVE E01 member for fingerprinting");
    let size = file.metadata().expect("read PVE E01 member metadata").len();
    let sample_len = usize::try_from(size.min(FINGERPRINT_SAMPLE_BYTES as u64))
        .expect("bounded fingerprint sample length");
    let mut first = vec![0u8; sample_len];
    file.read_exact(&mut first)
        .expect("read PVE E01 member prefix");
    let tail_offset = size.saturating_sub(sample_len as u64);
    file.seek(SeekFrom::Start(tail_offset))
        .expect("seek PVE E01 member suffix");
    let mut last = vec![0u8; sample_len];
    file.read_exact(&mut last)
        .expect("read PVE E01 member suffix");

    let mut hasher = Sha256::new();
    hasher.update(b"pve-host-e01-member-v1\0");
    hasher.update(size.to_le_bytes());
    hasher.update(first);
    hasher.update(last);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn discover_pve_root_ext4(member: &Path) -> ImageFilesystemCandidate {
    let mut reader = E01Reader::open(member).expect("open selected PVE disk01 E01 member");
    let mut probe = detect_image_filesystem(&mut reader).expect("probe selected PVE disk01 member");
    expand_lvm_pool_candidates(&mut probe, member, &DataSourceKind::E01);

    probe
        .candidates
        .into_iter()
        .find(|candidate| {
            candidate.kind == ImageFilesystemKind::Ext4
                && candidate.source == ImageFilesystemSource::LvmLogicalVolume
                && candidate
                    .lvm_identity
                    .as_ref()
                    .is_some_and(|identity| identity.vg_name == "pve" && identity.lv_name == "root")
        })
        .expect("selected PVE disk01 member must expose the EXT4 pve/root logical volume")
}

fn open_pve_root_ext4(member: &Path, candidate: &ImageFilesystemCandidate) -> Ext4Reader {
    let identity = candidate
        .lvm_identity
        .as_ref()
        .expect("pve/root candidate must retain LVM identity");
    let readers = if identity.pv_sources.is_empty() {
        vec![
            Box::new(E01Reader::open(member).expect("reopen selected PVE E01 member"))
                as Box<dyn EvidenceReader>,
        ]
    } else {
        identity
            .pv_sources
            .iter()
            .map(|source| {
                assert_eq!(
                    source.source_kind,
                    Some(DataSourceKind::E01),
                    "pve/root physical volume must remain bound to an E01 source"
                );
                Box::new(
                    E01Reader::open(Path::new(&source.source_path))
                        .expect("reopen pve/root physical-volume E01 source"),
                ) as Box<dyn EvidenceReader>
            })
            .collect()
    };
    let pool = fs_lvm::LvmPool::discover(readers, identity.pv_offsets.clone())
        .expect("reconstruct pve/root LVM pool");
    let volume_index = pool
        .list_volumes()
        .iter()
        .position(|volume| {
            (!identity.lv_uuid.is_empty() && volume.uuid == identity.lv_uuid)
                || (volume.name == identity.lv_name && identity.lv_name == "root")
        })
        .expect("find pve/root logical volume in reconstructed pool");
    let volume_reader = pool
        .open_volume_reader(volume_index)
        .expect("open pve/root as a read-only logical block device");
    Ext4Reader::open(volume_reader, 0).expect("open pve/root as EXT4")
}

fn select_target_file(fs: &Ext4Reader) -> TargetFile {
    let mut fallbacks = TargetFallbacks::default();
    if let Some(target) = find_target_in_directory(fs, "", 0, &mut fallbacks) {
        return target;
    }
    fallbacks
        .at_least_1_mib
        .or(fallbacks.at_least_64_kib)
        .expect("pve/root must contain a readable ordinary file of at least 64 KiB")
}

fn find_target_in_directory(
    fs: &Ext4Reader,
    directory: &str,
    depth: usize,
    fallbacks: &mut TargetFallbacks,
) -> Option<TargetFile> {
    if depth > MAX_DIRECTORY_DEPTH {
        return None;
    }
    let mut children = fs.list_children(directory).ok()?;
    children.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
            .then_with(|| left.name.cmp(&right.name))
    });

    for child in children {
        let path = join_logical_path(directory, &child.name);
        if child.is_dir {
            if let Some(target) = find_target_in_directory(fs, &path, depth + 1, fallbacks) {
                return Some(target);
            }
            continue;
        }
        if child.size < RANGE_64_KIB as u64 {
            continue;
        }
        let target = TargetFile {
            logical_path: path,
            size: child.size,
            selection_tier: if child.size >= (SEQUENTIAL_1_MIB_COUNT * RANGE_1_MIB) as u64 {
                "atLeast4MiB"
            } else if child.size >= (SEQUENTIAL_64_KIB_COUNT * RANGE_64_KIB) as u64 {
                "atLeast1MiB"
            } else {
                "atLeast64KiB"
            },
        };
        if target.size >= (SEQUENTIAL_1_MIB_COUNT * RANGE_1_MIB) as u64 {
            return Some(target);
        }
        if target.size >= (SEQUENTIAL_64_KIB_COUNT * RANGE_64_KIB) as u64 {
            fallbacks.at_least_1_mib.get_or_insert(target);
        } else {
            fallbacks.at_least_64_kib.get_or_insert(target);
        }
    }
    None
}

fn join_logical_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

fn read_exact_range(fs: &Ext4Reader, target: &TargetFile, offset: u64, length: usize) -> Vec<u8> {
    let bytes = fs
        .read_file_range(&target.logical_path, offset, length)
        .unwrap_or_else(|error| {
            panic!("read EXT4 logical file range offset={offset} length={length}: {error}")
        });
    assert_eq!(
        bytes.len(),
        length,
        "eligible preview scenario must return its full requested range"
    );
    bytes
}

fn measure_repeated_range(
    fs: &Ext4Reader,
    target: &TargetFile,
    offset: u64,
    length: usize,
    count: usize,
) -> (Vec<Duration>, String) {
    let mut timings = Vec::with_capacity(count);
    let mut expected_digest = None;
    for _ in 0..count {
        let started = Instant::now();
        let bytes = read_exact_range(fs, target, offset, length);
        timings.push(started.elapsed());
        let actual_digest = digest(&bytes);
        if let Some(expected) = &expected_digest {
            assert_eq!(
                &actual_digest, expected,
                "repeated native EXT4 reads must return identical evidence bytes"
            );
        } else {
            expected_digest = Some(actual_digest);
        }
    }
    (
        timings,
        expected_digest.expect("repeated preview range digest"),
    )
}

fn measure_sequential_or_skip(
    fs: &Ext4Reader,
    target: &TargetFile,
    scenario: &'static str,
    range_length: usize,
    count: usize,
    skipped_reason: &'static str,
) -> TimingMetric {
    let required_size = (range_length * count) as u64;
    if target.size < required_size {
        return skipped_metric(scenario, range_length, skipped_reason);
    }

    let mut timings = Vec::with_capacity(count);
    let mut hasher = Sha256::new();
    for index in 0..count {
        let offset = (index * range_length) as u64;
        let started = Instant::now();
        let bytes = read_exact_range(fs, target, offset, range_length);
        timings.push(started.elapsed());
        hasher.update(bytes);
    }
    measured_metric(
        scenario,
        range_length,
        &timings,
        Some(hex::encode(hasher.finalize())),
    )
}

fn measured_metric(
    scenario: &'static str,
    bytes_per_sample: usize,
    timings: &[Duration],
    digest_sha256: Option<String>,
) -> TimingMetric {
    assert!(!timings.is_empty(), "measured metric requires samples");
    let mut values = timings.iter().copied().map(duration_ms).collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    TimingMetric {
        scenario,
        status: "measured",
        samples: values.len(),
        bytes_per_sample,
        p50_ms: Some(percentile(&values, 0.50)),
        p95_ms: Some(percentile(&values, 0.95)),
        max_ms: values.last().copied(),
        digest_sha256,
        skipped_reason: None,
    }
}

fn skipped_metric(
    scenario: &'static str,
    bytes_per_sample: usize,
    skipped_reason: &'static str,
) -> TimingMetric {
    TimingMetric {
        scenario,
        status: "skipped",
        samples: 0,
        bytes_per_sample,
        p50_ms: None,
        p95_ms: None,
        max_ms: None,
        digest_sha256: None,
        skipped_reason: Some(skipped_reason),
    }
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn percentile(sorted: &[f64], quantile: f64) -> f64 {
    let rank = ((sorted.len() as f64 * quantile).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1);
    sorted[rank]
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn assert_sanitized_logical_path(path: &str) {
    assert!(!path.is_empty(), "logical preview path must not be empty");
    assert!(
        !path.contains('\\') && !Path::new(path).is_absolute(),
        "metrics must contain only an EXT4-relative logical path"
    );
}
