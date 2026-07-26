use std::{
    collections::VecDeque,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use app_services::import_analysis::{current_rss_mb, peak_rss_mb};
use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const FIXTURE_ENV: &str = "FORENSICS_LINUX_E01_FIXTURE";
const ORACLE_ENV: &str = "FORENSICS_LINUX_PREVIEW_ORACLE";
const DEFAULT_ORACLE_RELATIVE_PATH: &str =
    "../../testdata/real-samples/native-linux-xfs-preview-oracle.json";
const LVM_POOL_OFFSET: u64 = 1_074_790_400;
const EXPECTED_VG_NAME: &str = "cl";
const EXPECTED_LV_NAME: &str = "root";
const RANGE_64_KIB: usize = 64 * 1024;
const RANGE_1_MIB: usize = 1024 * 1024;
const REQUIRED_FILE_BYTES: u64 = 4 * 1024 * 1024;
const WARM_REPEAT_COUNT: usize = 12;
const MAX_SCANNED_DIRECTORIES: usize = 20_000;
#[derive(Debug)]
struct TargetFile {
    logical_path: String,
    size: u64,
}

#[derive(Debug)]
struct ReadSample {
    elapsed: Duration,
    actual_bytes: usize,
    sha256: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FixtureFingerprint {
    algorithm: &'static str,
    logical_size: u64,
    sha256: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileReport {
    logical_path: String,
    size: u64,
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
struct RangeOracleCandidate {
    scenario: &'static str,
    offset: u64,
    requested_bytes: usize,
    actual_bytes: usize,
    sha256: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OracleCapture {
    status: &'static str,
    ranges: Vec<RangeOracleCandidate>,
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
struct NativeXfsPreviewReport {
    schema_version: u32,
    fixture_fingerprint: FixtureFingerprint,
    volume: &'static str,
    file: FileReport,
    metrics: Vec<TimingMetric>,
    oracle_capture: OracleCapture,
    skipped_scenarios: Vec<&'static str>,
    memory: MemoryReport,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixedOracle {
    schema_version: u32,
    fixture_fingerprint: FixedFixtureFingerprint,
    volume: String,
    file: FixedFile,
    ranges: Vec<FixedRange>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixedFixtureFingerprint {
    algorithm: String,
    logical_size: u64,
    sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixedFile {
    logical_path: String,
    size: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixedRange {
    scenario: String,
    offset: u64,
    requested_bytes: usize,
    actual_bytes: usize,
    sha256: String,
}

#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"]
fn native_linux_xfs_preview_performance() {
    let fixture = required_fixture_path();
    let (fixture_fingerprint, target) = discover_target(&fixture);

    let rss_before_mb = current_rss_mb();
    let cold_total_started = Instant::now();
    let cold_open_started = Instant::now();
    let xfs = open_root_xfs(&fixture);
    let cold_open = cold_open_started.elapsed();
    let cold_head = timed_read(&xfs, &target.logical_path, 0, RANGE_64_KIB);
    let cold_open_read = cold_total_started.elapsed();

    let warm_same = (0..WARM_REPEAT_COUNT)
        .map(|_| timed_read(&xfs, &target.logical_path, 0, RANGE_64_KIB))
        .collect::<Vec<_>>();
    assert!(
        warm_same
            .iter()
            .all(|sample| sample.sha256 == cold_head.sha256),
        "warm reads changed the first 64 KiB evidence digest"
    );

    let sequential_64k = (0..16u64)
        .map(|index| {
            timed_read(
                &xfs,
                &target.logical_path,
                index * RANGE_64_KIB as u64,
                RANGE_64_KIB,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        sequential_64k[0].sha256, cold_head.sha256,
        "cold and sequential first-range evidence digests differ"
    );

    let sequential_1m = (0..4u64)
        .map(|index| {
            timed_read(
                &xfs,
                &target.logical_path,
                index * RANGE_1_MIB as u64,
                RANGE_1_MIB,
            )
        })
        .collect::<Vec<_>>();
    let rss_after_mb = current_rss_mb();

    let metrics = vec![
        metric("coldOpen", &[cold_open]),
        metric("coldRead64KiB", &[cold_head.elapsed]),
        metric("coldOpenRead64KiB", &[cold_open_read]),
        metric_from_samples("warmSame64KiB", &warm_same),
        metric_from_samples("sequential16x64KiB", &sequential_64k),
        metric_from_samples("sequential4x1MiB", &sequential_1m),
    ];
    let oracle_capture = OracleCapture {
        status: "fixed-oracle-verified",
        ranges: vec![
            oracle_candidate("head64KiB", 0, RANGE_64_KIB, &cold_head),
            oracle_candidate(
                "offset960KiB64KiB",
                15 * RANGE_64_KIB as u64,
                RANGE_64_KIB,
                &sequential_64k[15],
            ),
            oracle_candidate("head1MiB", 0, RANGE_1_MIB, &sequential_1m[0]),
            oracle_candidate(
                "offset3MiB1MiB",
                3 * RANGE_1_MIB as u64,
                RANGE_1_MIB,
                &sequential_1m[3],
            ),
        ],
    };
    let report = NativeXfsPreviewReport {
        schema_version: 1,
        fixture_fingerprint,
        volume: "cl/root",
        file: FileReport {
            logical_path: target.logical_path,
            size: target.size,
        },
        metrics,
        oracle_capture,
        skipped_scenarios: Vec::new(),
        memory: MemoryReport {
            rss_before_mb,
            rss_after_mb,
            rss_delta_mb: rss_after_mb as i64 - rss_before_mb as i64,
            peak_rss_mb: peak_rss_mb(),
        },
    };
    validate_fixed_oracle(&report);

    eprintln!(
        "NATIVE_XFS_PREVIEW_METRICS {}",
        serde_json::to_string(&report).expect("serialize native XFS preview metrics")
    );
}

fn validate_fixed_oracle(report: &NativeXfsPreviewReport) {
    let oracle_json = required_oracle_json();
    let expected: FixedOracle =
        serde_json::from_str(&oracle_json).expect("parse native XFS preview oracle");
    assert_eq!(expected.schema_version, report.schema_version);
    assert_eq!(
        expected.fixture_fingerprint.algorithm,
        report.fixture_fingerprint.algorithm
    );
    assert_eq!(
        expected.fixture_fingerprint.logical_size,
        report.fixture_fingerprint.logical_size
    );
    assert_eq!(
        expected.fixture_fingerprint.sha256,
        report.fixture_fingerprint.sha256
    );
    assert_eq!(expected.volume, report.volume);
    assert_eq!(expected.file.logical_path, report.file.logical_path);
    assert_eq!(expected.file.size, report.file.size);
    assert_eq!(expected.ranges.len(), report.oracle_capture.ranges.len());
    for expected_range in expected.ranges {
        let actual = report
            .oracle_capture
            .ranges
            .iter()
            .find(|candidate| candidate.scenario == expected_range.scenario)
            .unwrap_or_else(|| {
                panic!(
                    "missing fixed native XFS range '{}'",
                    expected_range.scenario
                )
            });
        assert_eq!(actual.offset, expected_range.offset);
        assert_eq!(actual.requested_bytes, expected_range.requested_bytes);
        assert_eq!(actual.actual_bytes, expected_range.actual_bytes);
        assert_eq!(actual.sha256, expected_range.sha256);
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
            "{ORACLE_ENV} must point to the native Linux preview oracle, or place it at {}: {error}",
            path.display()
        )
    })
}

fn required_fixture_path() -> PathBuf {
    let fixture = std::env::var_os(FIXTURE_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("{FIXTURE_ENV} must point to the real Linux E01 sample"));
    assert!(
        fixture.is_file(),
        "{FIXTURE_ENV} must point to an existing E01 file"
    );
    fixture
}

fn discover_target(fixture: &Path) -> (FixtureFingerprint, TargetFile) {
    let mut e01 = E01Reader::open(fixture).expect("open Linux E01 fixture for target discovery");
    let fingerprint = fingerprint_fixture(&mut e01);
    let xfs = open_root_xfs_from_e01(e01);
    let target = select_stable_target(&xfs);
    (fingerprint, target)
}

fn fingerprint_fixture(reader: &mut E01Reader) -> FixtureFingerprint {
    let logical_size = reader.info().size;
    assert!(
        logical_size >= RANGE_64_KIB as u64,
        "Linux E01 logical evidence is unexpectedly small"
    );
    let tail_offset = logical_size.saturating_sub(RANGE_64_KIB as u64);
    let offsets = [0, logical_size / 2, tail_offset];
    let mut hasher = Sha256::new();
    hasher.update(b"Meow_Detective/native-linux-e01-fingerprint/v1");
    hasher.update(logical_size.to_le_bytes());
    for offset in offsets {
        reader
            .seek(SeekFrom::Start(offset))
            .expect("seek logical E01 fingerprint range");
        let mut bytes = vec![0u8; RANGE_64_KIB];
        reader
            .read_exact(&mut bytes)
            .expect("read logical E01 fingerprint range");
        hasher.update(offset.to_le_bytes());
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    FixtureFingerprint {
        algorithm: "sha256-logical-e01-samples-v1",
        logical_size,
        sha256: hex::encode(hasher.finalize()),
    }
}

fn open_root_xfs(fixture: &Path) -> fs_xfs::XfsReader {
    let e01 = E01Reader::open(fixture).expect("open Linux E01 fixture");
    open_root_xfs_from_e01(e01)
}

fn open_root_xfs_from_e01(e01: E01Reader) -> fs_xfs::XfsReader {
    let pool = fs_lvm::LvmPool::discover(
        vec![Box::new(e01) as Box<dyn EvidenceReader>],
        vec![LVM_POOL_OFFSET],
    )
    .expect("discover Linux LVM pool");
    assert_eq!(
        pool.volume_group().name,
        EXPECTED_VG_NAME,
        "real Linux fixture must expose the expected LVM volume group"
    );
    let root_index = pool
        .list_volumes()
        .iter()
        .position(|volume| volume.name == EXPECTED_LV_NAME)
        .expect("real Linux fixture must expose cl/root");
    let root_volume = pool
        .open_volume(root_index)
        .expect("open cl/root as a read-only logical volume");
    fs_xfs::XfsReader::open(Box::new(root_volume), 0)
        .expect("open cl/root as a read-only XFS filesystem")
}

fn select_stable_target(xfs: &dyn FileSystemReader) -> TargetFile {
    let mut pending = VecDeque::from([String::new()]);
    let mut scanned_directories = 0usize;
    let mut largest_seen = 0u64;

    while let Some(directory) = pending.pop_front() {
        scanned_directories += 1;
        assert!(
            scanned_directories <= MAX_SCANNED_DIRECTORIES,
            "no suitable regular file found within the bounded XFS tree scan; largest_seen={largest_seen}"
        );
        let Ok(mut children) = xfs.list_children(&directory) else {
            continue;
        };
        children.sort_by(|left, right| left.path.cmp(&right.path));

        for child in children.iter().filter(|child| !child.is_dir) {
            largest_seen = largest_seen.max(child.size);
            if child.size < REQUIRED_FILE_BYTES {
                continue;
            }
            if target_supports_full_matrix(xfs, &child.path) {
                return TargetFile {
                    logical_path: child.path.clone(),
                    size: child.size,
                };
            }
        }
        pending.extend(
            children
                .into_iter()
                .filter(|child| child.is_dir)
                .map(|child| child.path),
        );
    }

    panic!(
        "real cl/root XFS tree has no readable regular file large enough for the 4 MiB preview matrix; largest_seen={largest_seen}"
    );
}

fn target_supports_full_matrix(xfs: &dyn FileSystemReader, path: &str) -> bool {
    [(0, RANGE_64_KIB), (3 * RANGE_1_MIB as u64, RANGE_1_MIB)]
        .into_iter()
        .all(|(offset, length)| {
            xfs.read_file_range(path, offset, length)
                .is_ok_and(|bytes| bytes.len() == length)
        })
}

fn timed_read(
    xfs: &dyn FileSystemReader,
    logical_path: &str,
    offset: u64,
    length: usize,
) -> ReadSample {
    let started = Instant::now();
    let bytes = xfs
        .read_file_range(logical_path, offset, length)
        .unwrap_or_else(|error| {
            panic!(
                "native XFS range read failed for logical path '{logical_path}' offset={offset} length={length}: {error}"
            )
        });
    let elapsed = started.elapsed();
    assert_eq!(
        bytes.len(),
        length,
        "native XFS range read was truncated for logical path '{logical_path}' offset={offset}"
    );
    ReadSample {
        elapsed,
        actual_bytes: bytes.len(),
        sha256: hex::encode(Sha256::digest(&bytes)),
    }
}

fn metric_from_samples(scenario: &'static str, samples: &[ReadSample]) -> TimingMetric {
    let durations = samples
        .iter()
        .map(|sample| sample.elapsed)
        .collect::<Vec<_>>();
    metric(scenario, &durations)
}

fn metric(scenario: &'static str, samples: &[Duration]) -> TimingMetric {
    assert!(!samples.is_empty(), "timing metric requires samples");
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

fn oracle_candidate(
    scenario: &'static str,
    offset: u64,
    requested_bytes: usize,
    sample: &ReadSample,
) -> RangeOracleCandidate {
    RangeOracleCandidate {
        scenario,
        offset,
        requested_bytes,
        actual_bytes: sample.actual_bytes,
        sha256: sample.sha256.clone(),
    }
}
