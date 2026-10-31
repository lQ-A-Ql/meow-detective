use super::*;
use evidence_core::EvidenceReader;
use std::io::{Cursor, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicBool, Ordering};

#[allow(dead_code, clippy::duplicate_mod)]
#[path = "../support/import_ntfs_fixture.rs"]
mod import_ntfs_fixture;

struct ShortReader(&'static [u8]);

impl Read for ShortReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = buffer.len().min(self.0.len());
        buffer[..count].copy_from_slice(&self.0[..count]);
        self.0 = &self.0[count..];
        Ok(count)
    }
}

#[test]
fn sha256_reader_basic() {
    let data = b"test data for hashing";
    let mut cursor = Cursor::new(data);
    let hash = HashService::sha256_reader(&mut cursor).unwrap();
    assert_eq!(hash, HashService::sha256_bytes(data));
}

#[test]
fn digest_file_supports_all_configured_algorithms() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("evidence.bin");
    std::fs::write(&path, b"abc").unwrap();
    let cases = [
        (
            infrastructure::hashing::HashAlgorithm::Md5,
            "900150983cd24fb0d6963f7d28e17f72",
        ),
        (
            infrastructure::hashing::HashAlgorithm::Sha1,
            "a9993e364706816aba3e25717850c26c9cd0d89d",
        ),
        (
            infrastructure::hashing::HashAlgorithm::Sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            infrastructure::hashing::HashAlgorithm::Sm3,
            "66c7f0f462eeedd9d1f2d46bdc10e4e24167c4875cf2f7a2297da02b8f4ba8e0",
        ),
    ];
    for (algorithm, expected) in cases {
        assert_eq!(
            HashService::digest_file(&path, algorithm).unwrap(),
            expected
        );
    }
}

#[test]
fn sha256_bytes_hello_world() {
    let hash = HashService::sha256_bytes(b"hello world");
    assert_eq!(
        hash,
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
}

#[test]
fn verify_sha256_correct() {
    let data = b"evidence data";
    let hash = HashService::sha256_bytes(data);
    assert!(HashService::verify_sha256(data, &hash));
}

#[test]
fn verify_sha256_incorrect() {
    assert!(!HashService::verify_sha256(
        b"hello",
        &HashService::sha256_bytes(b"world")
    ));
}

#[test]
fn sha256_file_nonexistent() {
    let result = HashService::sha256_file(Path::new("/nonexistent/file"));
    assert!(result.is_err());
}

#[test]
fn hash_evidence_raw_is_standard_sha256_and_reports_backend() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("evidence.raw");
    std::fs::write(&path, b"raw evidence").unwrap();
    let cancelled = AtomicBool::new(false);
    let progress = AtomicBool::new(false);
    let result =
        HashService::hash_evidence(&path, &domain::DataSourceKind::Raw, &cancelled, &|_, _| {
            progress.store(true, Ordering::Release)
        })
        .unwrap();
    assert_eq!(result.digest, HashService::sha256_bytes(b"raw evidence"));
    assert_eq!(result.bytes_processed, 12);
    assert_eq!(result.parallel_segments, 1);
    assert_eq!(
        result.worker_threads,
        infrastructure::hashing::sha256_pipeline_worker_threads()
    );
    assert!(matches!(result.acceleration, "sha-ni" | "portable"));
    assert!(progress.load(Ordering::Acquire));
}

#[test]
fn hash_evidence_e01_uses_deterministic_manifest_for_multiple_segments() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("sample.E01");
    let second = directory.path().join("sample.E02");
    std::fs::write(&first, b"first").unwrap();
    std::fs::write(&second, b"second").unwrap();
    let cancelled = AtomicBool::new(false);
    let result =
        HashService::hash_evidence(&first, &domain::DataSourceKind::E01, &cancelled, &|_, _| {})
            .unwrap();
    let manifest = format!(
        "segment=00000000;length=5;sha256={}\nsegment=00000001;length=6;sha256={}\n",
        HashService::sha256_bytes(b"first"),
        HashService::sha256_bytes(b"second")
    );
    assert_eq!(
        result.digest,
        HashService::sha256_bytes(manifest.as_bytes())
    );
    assert_eq!(result.bytes_processed, 11);
    assert_eq!(result.parallel_segments, 2);
    assert!(result.worker_threads >= 1);
}

#[test]
fn hash_evidence_vmdk_covers_descriptor_and_flat_extent() {
    let directory = tempfile::tempdir().unwrap();
    let descriptor = directory.path().join("disk.vmdk");
    let extent = directory.path().join("disk-flat.vmdk");
    let descriptor_bytes = b"# Disk DescriptorFile\nparentCID=ffffffff\ncreateType=\"monolithicFlat\"\nRW 1 FLAT \"disk-flat.vmdk\" 0\n";
    let extent_bytes = vec![0xA5; 512];
    std::fs::write(&descriptor, descriptor_bytes).unwrap();
    std::fs::write(&extent, &extent_bytes).unwrap();

    let cancelled = AtomicBool::new(false);
    let result = HashService::hash_evidence(
        &descriptor,
        &domain::DataSourceKind::Raw,
        &cancelled,
        &|_, _| {},
    )
    .unwrap();
    let manifest = format!(
        "segment=00000000;length={};sha256={}\nsegment=00000001;length=512;sha256={}\n",
        descriptor_bytes.len(),
        HashService::sha256_bytes(descriptor_bytes),
        HashService::sha256_bytes(&extent_bytes),
    );
    assert_eq!(
        result.digest,
        HashService::sha256_bytes(manifest.as_bytes())
    );
    assert_eq!(result.parallel_segments, 2);
    assert_eq!(
        result.bytes_processed,
        descriptor_bytes.len() as u64 + extent_bytes.len() as u64
    );
}

#[test]
fn hash_evidence_honors_cancellation_before_reading() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("evidence.raw");
    std::fs::write(&path, b"evidence").unwrap();
    let cancelled = AtomicBool::new(true);
    let error =
        HashService::hash_evidence(&path, &domain::DataSourceKind::Raw, &cancelled, &|_, _| {})
            .unwrap_err();
    assert!(matches!(error, EvidenceHashError::Cancelled));
}

#[test]
fn exact_reader_digest_rejects_early_eof() {
    let mut reader = ShortReader(b"short");
    let cancelled = AtomicBool::new(false);
    let error = digest_reader_exact(
        &mut reader,
        infrastructure::hashing::HashAlgorithm::Sha256,
        10,
        &cancelled,
        |_| {},
    )
    .unwrap_err();

    assert!(matches!(
        error,
        EvidenceHashError::Io {
            operation: "read complete evidence input",
            kind: io::ErrorKind::UnexpectedEof
        }
    ));
}

#[test]
fn directory_hash_honors_cancellation_during_enumeration() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("evidence.txt"), b"evidence").unwrap();
    let cancelled = AtomicBool::new(true);

    let error = hash_directory_evidence(
        directory.path(),
        infrastructure::hashing::HashAlgorithm::Sha256,
        &cancelled,
        &|_, _| {},
    )
    .unwrap_err();

    assert!(matches!(error, EvidenceHashError::Cancelled));
}

#[test]
fn directory_hash_enforces_the_entry_limit() {
    assert!(check_directory_entry_limit(1_000_000).is_ok());
    assert!(matches!(
        check_directory_entry_limit(1_000_001),
        Err(EvidenceHashError::DirectoryEntryLimit)
    ));
}

#[test]
fn e01_segment_discovery_rejects_a_missing_middle_segment() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("sample.E01"), b"first").unwrap();
    std::fs::write(directory.path().join("sample.E03"), b"third").unwrap();

    let error = volumes::discover_e01_segments(&directory.path().join("sample.E01")).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn ewf_first_segment_accepts_contiguous_e_numbered_parts() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("sample.EWF");
    std::fs::write(&first, b"first").unwrap();
    std::fs::write(directory.path().join("sample.E02"), b"second").unwrap();

    let segments = volumes::discover_e01_segments(&first).unwrap();
    assert_eq!(segments, vec![first, directory.path().join("sample.E02")]);
}

#[test]
fn public_e01_and_raw_fixtures_support_all_digest_algorithms() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cancelled = AtomicBool::new(false);
    for (kind, relative) in [
        (
            domain::DataSourceKind::E01,
            "testdata/fixtures/public-small/e01/tiny.E01",
        ),
        (
            domain::DataSourceKind::Raw,
            "testdata/fixtures/public-small/raw/tiny.raw",
        ),
    ] {
        let path = root.join(relative);
        for algorithm in [
            infrastructure::hashing::HashAlgorithm::Md5,
            infrastructure::hashing::HashAlgorithm::Sha1,
            infrastructure::hashing::HashAlgorithm::Sha256,
            infrastructure::hashing::HashAlgorithm::Sm3,
        ] {
            let result =
                HashService::hash_logical_evidence(&path, &kind, algorithm, &cancelled, &|_, _| {})
                    .unwrap();
            assert_eq!(
                result.digest.len(),
                match algorithm {
                    infrastructure::hashing::HashAlgorithm::Md5 => 32,
                    infrastructure::hashing::HashAlgorithm::Sha1 => 40,
                    infrastructure::hashing::HashAlgorithm::Sha256
                    | infrastructure::hashing::HashAlgorithm::Sm3 => 64,
                }
            );
        }
    }
}

#[test]
fn synthetic_multichunk_e01_matches_known_content_for_all_algorithms() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("multichunk.E01");
    let bytes: Vec<u8> = (0..5 * 1024 * 1024)
        .map(|index| ((index * 31 + index / 251) & 0xff) as u8)
        .collect();
    import_ntfs_fixture::write_e01(&path, &bytes).unwrap();
    assert_eq!(
        image_e01::E01Reader::open(&path).unwrap().info().size,
        bytes.len() as u64
    );

    let cancelled = AtomicBool::new(false);
    for algorithm in [
        infrastructure::hashing::HashAlgorithm::Md5,
        infrastructure::hashing::HashAlgorithm::Sha1,
        infrastructure::hashing::HashAlgorithm::Sha256,
        infrastructure::hashing::HashAlgorithm::Sm3,
    ] {
        let result = HashService::hash_logical_evidence(
            &path,
            &domain::DataSourceKind::E01,
            algorithm,
            &cancelled,
            &|_, _| {},
        )
        .unwrap();
        assert_eq!(
            result.digest,
            infrastructure::hashing::digest_bytes(&bytes, algorithm)
        );
        assert_eq!(result.bytes_processed, bytes.len() as u64);
        assert!(result.worker_threads >= 1);
    }
}

#[test]
fn e01_digest_lanes_scale_with_cpu_and_memory_headroom() {
    let eight_gib = 8 * 1024 * 1024 * 1024;
    assert_eq!(e01_pipeline::digest_lane_count(eight_gib, 4, 100, 1536), 8);
    assert_eq!(
        e01_pipeline::digest_lane_count(eight_gib, 64, 100, 1536),
        32
    );
    assert_eq!(
        e01_pipeline::digest_lane_count(eight_gib, 64, 1520, 1536),
        4
    );
    assert_eq!(
        e01_pipeline::digest_lane_count(eight_gib, 64, 1536, 1536),
        1
    );
}

#[test]
fn e01_digest_lanes_are_bounded_by_the_logical_length() {
    assert_eq!(e01_pipeline::digest_lane_count(1, 64, 100, 1536), 1);
    assert_eq!(
        e01_pipeline::digest_lane_count(5 * 1024 * 1024, 64, 100, 1536),
        3
    );
}

#[test]
fn public_e01_rejects_a_truncated_tail() {
    let directory = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/e01/tiny.E01");
    let path = directory.path().join("truncated.E01");
    std::fs::copy(&source, &path).unwrap();
    let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    let length = file.metadata().unwrap().len();
    file.set_len(length - 1).unwrap();
    file.sync_all().unwrap();
    let cancelled = AtomicBool::new(false);

    let error = HashService::hash_logical_evidence(
        &path,
        &domain::DataSourceKind::E01,
        infrastructure::hashing::HashAlgorithm::Sha256,
        &cancelled,
        &|_, _| {},
    )
    .unwrap_err();

    assert!(matches!(error, EvidenceHashError::Io { .. }));
}

#[test]
fn public_e01_rejects_an_appended_segment() {
    let directory = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/e01/tiny.E01");
    let path = directory.path().join("appended.E01");
    std::fs::copy(&source, &path).unwrap();
    std::fs::write(directory.path().join("appended.E02"), b"unexpected segment").unwrap();
    let cancelled = AtomicBool::new(false);

    let error = HashService::hash_logical_evidence(
        &path,
        &domain::DataSourceKind::E01,
        infrastructure::hashing::HashAlgorithm::Sha256,
        &cancelled,
        &|_, _| {},
    )
    .unwrap_err();

    assert!(matches!(error, EvidenceHashError::Io { .. }));
}

#[test]
#[ignore = "set FORENSICS_VHDX_FIXTURE to validate all VHDX logical digest algorithms"]
fn vhdx_fixture_supports_all_digest_algorithms() {
    let path = std::env::var_os("FORENSICS_VHDX_FIXTURE")
        .map(std::path::PathBuf::from)
        .expect("set FORENSICS_VHDX_FIXTURE");
    let cancelled = AtomicBool::new(false);
    for algorithm in [
        infrastructure::hashing::HashAlgorithm::Md5,
        infrastructure::hashing::HashAlgorithm::Sha1,
        infrastructure::hashing::HashAlgorithm::Sha256,
        infrastructure::hashing::HashAlgorithm::Sm3,
    ] {
        HashService::hash_logical_evidence(
            &path,
            &domain::DataSourceKind::Raw,
            algorithm,
            &cancelled,
            &|_, _| {},
        )
        .unwrap();
    }
}

#[test]
#[ignore = "set FORENSICS_E01_FIXTURE to validate all E01 logical digest algorithms"]
fn e01_fixture_supports_all_digest_algorithms() {
    let path = std::env::var_os("FORENSICS_E01_FIXTURE")
        .map(std::path::PathBuf::from)
        .expect("set FORENSICS_E01_FIXTURE");
    let cancelled = AtomicBool::new(false);
    let logical_bytes = image_e01::E01Reader::open(&path).unwrap().info().size;
    eprintln!("E01_LOGICAL_INFO bytes={logical_bytes}");
    for algorithm in [
        infrastructure::hashing::HashAlgorithm::Md5,
        infrastructure::hashing::HashAlgorithm::Sha1,
        infrastructure::hashing::HashAlgorithm::Sha256,
        infrastructure::hashing::HashAlgorithm::Sm3,
    ] {
        let started = std::time::Instant::now();
        let last_report = AtomicU64::new(0);
        let progress = |processed: u64, total: u64| {
            const REPORT_STEP: u64 = 1024 * 1024 * 1024;
            let previous = last_report.load(Ordering::Acquire);
            if processed.saturating_sub(previous) < REPORT_STEP || processed == total {
                return;
            }
            last_report.store(processed, Ordering::Release);
            eprintln!(
                "E01_LOGICAL_PROGRESS algorithm={} processed={} total={} elapsed_ms={} throughput_mib_s={}",
                algorithm.label(),
                processed,
                total,
                started.elapsed().as_millis(),
                processed as f64 / started.elapsed().as_secs_f64() / (1024.0 * 1024.0)
            );
        };
        let result = HashService::hash_logical_evidence(
            &path,
            &domain::DataSourceKind::E01,
            algorithm,
            &cancelled,
            &progress,
        )
        .unwrap();
        eprintln!(
            "E01_LOGICAL algorithm={} bytes={} workers={} elapsed_ms={} digest={}",
            algorithm.label(),
            result.bytes_processed,
            result.worker_threads,
            started.elapsed().as_millis(),
            result.digest
        );
    }
}

#[test]
#[ignore = "set FORENSICS_E01_FIXTURE to validate deterministic E01 SHA-256"]
fn e01_fixture_sha256_is_deterministic() {
    let path = std::env::var_os("FORENSICS_E01_FIXTURE")
        .map(std::path::PathBuf::from)
        .expect("set FORENSICS_E01_FIXTURE");
    let cancelled = AtomicBool::new(false);
    let first = HashService::hash_logical_evidence(
        &path,
        &domain::DataSourceKind::E01,
        infrastructure::hashing::HashAlgorithm::Sha256,
        &cancelled,
        &|_, _| {},
    )
    .unwrap();
    let second = HashService::hash_logical_evidence(
        &path,
        &domain::DataSourceKind::E01,
        infrastructure::hashing::HashAlgorithm::Sha256,
        &cancelled,
        &|_, _| {},
    )
    .unwrap();
    assert_eq!(first.digest, second.digest);
    assert_eq!(first.bytes_processed, second.bytes_processed);
}

#[test]
#[ignore = "set FORENSICS_LOCAL_DISK_PATH to validate all physical-disk digest algorithms"]
fn local_disk_fixture_supports_all_digest_algorithms() {
    let path = std::env::var_os("FORENSICS_LOCAL_DISK_PATH")
        .map(std::path::PathBuf::from)
        .expect("set FORENSICS_LOCAL_DISK_PATH");
    let cancelled = AtomicBool::new(false);
    for algorithm in [
        infrastructure::hashing::HashAlgorithm::Md5,
        infrastructure::hashing::HashAlgorithm::Sha1,
        infrastructure::hashing::HashAlgorithm::Sha256,
        infrastructure::hashing::HashAlgorithm::Sm3,
    ] {
        HashService::hash_logical_evidence(
            &path,
            &domain::DataSourceKind::LocalDisk,
            algorithm,
            &cancelled,
            &|_, _| {},
        )
        .unwrap();
    }
}

#[test]
fn hash_evidence_rejects_same_length_changes_during_read() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("changing.raw");
    std::fs::write(&path, vec![0x41u8; 2 * 1024 * 1024]).unwrap();
    let changed = AtomicBool::new(false);
    let cancelled = AtomicBool::new(false);

    let error = HashService::hash_evidence_with_algorithm(
        &path,
        &domain::DataSourceKind::Raw,
        infrastructure::hashing::HashAlgorithm::Md5,
        &cancelled,
        &|processed, _| {
            if processed == 0 || changed.swap(true, Ordering::AcqRel) {
                return;
            }
            let mut file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
            file.seek(SeekFrom::Start(0)).unwrap();
            file.write_all(b"changed").unwrap();
            file.sync_all().unwrap();
        },
    )
    .unwrap_err();

    assert!(matches!(error, EvidenceHashError::EvidenceChanged));
}

#[test]
fn hash_evidence_rejects_segment_growth_during_read() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("growing.raw");
    std::fs::write(&path, vec![0u8; 2 * 1024 * 1024]).unwrap();
    let changed = AtomicBool::new(false);
    let cancelled = AtomicBool::new(false);

    let error = HashService::hash_evidence_with_algorithm(
        &path,
        &domain::DataSourceKind::Raw,
        infrastructure::hashing::HashAlgorithm::Md5,
        &cancelled,
        &|processed, _| {
            if processed == 0 || changed.swap(true, Ordering::AcqRel) {
                return;
            }
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            file.write_all(b"x").unwrap();
            file.sync_all().unwrap();
        },
    )
    .unwrap_err();

    assert!(matches!(error, EvidenceHashError::EvidenceChanged));
}

#[test]
fn hash_evidence_directory_is_deterministic_and_includes_file_content() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("nested")).unwrap();
    std::fs::write(directory.path().join("z.txt"), b"z").unwrap();
    std::fs::write(directory.path().join("nested").join("a.txt"), b"a").unwrap();
    let cancelled = AtomicBool::new(false);
    let first = HashService::hash_evidence_with_algorithm(
        directory.path(),
        &domain::DataSourceKind::LogicalDirectory,
        infrastructure::hashing::HashAlgorithm::Sha256,
        &cancelled,
        &|_, _| {},
    )
    .unwrap();
    std::fs::rename(
        directory.path().join("z.txt"),
        directory.path().join("renamed.txt"),
    )
    .unwrap();
    let second = HashService::hash_evidence_with_algorithm(
        directory.path(),
        &domain::DataSourceKind::LogicalDirectory,
        infrastructure::hashing::HashAlgorithm::Sha256,
        &cancelled,
        &|_, _| {},
    )
    .unwrap();
    assert_ne!(first.digest, second.digest);
    assert_eq!(first.bytes_processed, 2);
}
