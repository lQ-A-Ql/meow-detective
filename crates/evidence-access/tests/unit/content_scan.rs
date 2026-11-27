use super::*;
use crate::source_runtime::content::{capture_factory, scan};
use domain::DataSourceKind;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::{
    fs::File,
    io::{Read, Write},
};

#[test]
fn real_readers_run_at_least_four_lanes_and_accept_more_than_twelve() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("content.raw");
    let oracle = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/fixtures/public-small/mdraid/oracle.bin"),
    )
    .unwrap();
    let mut file = File::create(&path).unwrap();
    for _ in 0..128 {
        file.write_all(&oracle).unwrap();
    }
    drop(file);
    let factory = crate::source_runtime::open_factory(&path, &DataSourceKind::Raw).unwrap();
    let independent = crate::hash_service::HashService::sha256_file(&path).unwrap();
    let mut signatures = Vec::new();
    for requested in [4, 24] {
        let result = scan::scan(&factory, None, &AtomicBool::new(false), requested).unwrap();
        assert_eq!(result.workers, requested);
        eprintln!(
            "content workers: requested={requested} actual={} peak_readers={}",
            result.workers, result.peak_readers
        );
        assert!(
            result.peak_readers >= 4,
            "observed peak readers {}",
            result.peak_readers
        );
        assert_eq!(result.digest, independent);
        assert_eq!(result.chunks.len(), 32);
        signatures.push(result.chunks);
    }
    assert_eq!(signatures[0], signatures[1]);
    assert!(matches!(
        scan::scan(&factory, None, &AtomicBool::new(false), 3),
        Err(ContentError::InvalidWorkerCount)
    ));
}

#[test]
#[ignore = "requires FORENSICS_PVE_CLUSTER_ROOT private E01 sample; reads a bounded 64 MiB prefix"]
fn private_e01_four_and_sixteen_workers_match_independent_prefix_digest() {
    use evidence_core::{EvidenceReaderFactory, ReaderCapabilities};
    struct Prefix(evidence_core::SharedReaderFactory);
    impl EvidenceReaderFactory for Prefix {
        fn open_reader(&self) -> std::io::Result<Box<dyn EvidenceReader>> {
            Ok(Box::new(evidence_core::PartitionWindowReader::new(
                self.0.open_reader()?,
                0,
                Some(self.logical_size()),
            )?))
        }
        fn logical_size(&self) -> u64 {
            self.0.logical_size().min(64 * 1024 * 1024)
        }
        fn preferred_read_granularity(&self) -> usize {
            self.0.preferred_read_granularity()
        }
        fn capabilities(&self) -> ReaderCapabilities {
            self.0.capabilities()
        }
        fn validate_snapshot(&self) -> std::io::Result<()> {
            self.0.validate_snapshot()
        }
    }
    let root = std::path::PathBuf::from(
        std::env::var_os("FORENSICS_PVE_CLUSTER_ROOT").expect("set private collection root"),
    );
    let path = root.join("server01/server01-disk01.E01");
    let inner = crate::source_runtime::open_factory(&path, &DataSourceKind::E01).unwrap();
    let factory: SharedReaderFactory = Arc::new(Prefix(inner));
    let mut independent = image_e01::E01Reader::open(&path)
        .unwrap()
        .take(factory.logical_size());
    let expected = crate::hash_service::HashService::sha256_reader(&mut independent).unwrap();
    for requested in [4, 16] {
        let started = std::time::Instant::now();
        let result = scan::scan(&factory, None, &AtomicBool::new(false), requested).unwrap();
        eprintln!(
            "private E01 bounded scan: bytes={} workers={} peak_readers={} elapsed_ms={}",
            result.size,
            result.workers,
            result.peak_readers,
            started.elapsed().as_millis()
        );
        assert_eq!(result.workers, requested);
        assert!(result.peak_readers >= 4);
        assert_eq!(result.digest, expected);
    }
}

#[test]
fn native_compressed_e01_parallel_scan_preserves_order_and_corruption_rejection() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/e01/tiny.E01");
    let mut source = image_e01::E01Reader::open(&fixture).unwrap();
    let mut bytes = Vec::new();
    source.read_to_end(&mut bytes).unwrap();
    let factory = crate::source_runtime::open_factory(&fixture, &DataSourceKind::E01).unwrap();
    let result = scan::scan(&factory, None, &AtomicBool::new(false), 4).unwrap();
    assert_eq!(result.digest, hex::encode(Sha256::digest(&bytes)));
    let mut identity = capture_factory(
        &fixture,
        &DataSourceKind::E01,
        &factory,
        &AtomicBool::new(false),
    )
    .unwrap();
    identity
        .verify_factory(&factory, &AtomicBool::new(false))
        .unwrap();
    identity.chunk_sha256[0] = "0".repeat(64);
    assert!(matches!(
        identity.verify_factory(&factory, &AtomicBool::new(false)),
        Err(ContentError::Changed)
    ));
    assert!(matches!(
        scan::scan(&factory, None, &AtomicBool::new(true), 4),
        Err(ContentError::Cancelled)
    ));
}

#[test]
fn running_parallel_scan_cancels_and_releases_all_reader_and_buffer_permits() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cancel.raw");
    File::create(&path)
        .unwrap()
        .set_len(512 * 1024 * 1024)
        .unwrap();
    let factory = crate::source_runtime::open_factory(&path, &DataSourceKind::Raw).unwrap();
    let before = source_kernel::resource_budget::global_pool().snapshot().0;
    let cancel = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(25));
            cancel.store(true, Ordering::Release);
        });
        assert!(matches!(
            scan::scan(&factory, None, &cancel, 4),
            Err(ContentError::Cancelled)
        ));
    });
    assert_eq!(
        source_kernel::resource_budget::global_pool().snapshot().0,
        before
    );
}

#[test]
#[ignore = "requires FORENSICS_PVE_CLUSTER_ROOT; bounded 2 GiB E01 stage profile"]
fn private_pve_content_stage_profile() {
    use evidence_core::{EvidenceReaderFactory, ReaderCapabilities};
    struct Prefix(SharedReaderFactory);
    impl EvidenceReaderFactory for Prefix {
        fn open_reader(&self) -> std::io::Result<Box<dyn EvidenceReader>> {
            Ok(Box::new(evidence_core::PartitionWindowReader::new(
                self.0.open_reader()?,
                0,
                Some(self.logical_size()),
            )?))
        }
        fn logical_size(&self) -> u64 {
            self.0.logical_size().min(2 * 1024 * 1024 * 1024)
        }
        fn preferred_read_granularity(&self) -> usize {
            self.0.preferred_read_granularity()
        }
        fn capabilities(&self) -> ReaderCapabilities {
            self.0.capabilities()
        }
        fn validate_snapshot(&self) -> std::io::Result<()> {
            self.0.validate_snapshot()
        }
    }
    let _logging = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(std::io::stderr)
            .finish(),
    );
    let root = std::path::PathBuf::from(
        std::env::var_os("FORENSICS_PVE_CLUSTER_ROOT").expect("set private collection root"),
    );
    let path = root.join("server03/server03-disk02.E01");
    let inner = crate::source_runtime::open_factory(&path, &DataSourceKind::E01).unwrap();
    let factory: SharedReaderFactory = Arc::new(Prefix(inner.clone()));
    let mut independent = image_e01::E01Reader::open(&path)
        .unwrap()
        .without_chunk_cache();
    let native_chunk_bytes = independent.preferred_read_granularity() as u64;
    let mut digest = Sha256::new();
    let mut buffer = vec![0; CONTENT_CHUNK_BYTES];
    for _ in 0..factory.logical_size() / CONTENT_CHUNK_BYTES as u64 {
        independent.read_exact(&mut buffer).unwrap();
        digest.update(&buffer);
    }
    let expected = hex::encode(digest.finalize());
    {
        let factory: SharedReaderFactory = Arc::new(Prefix(inner.clone()));
        for workers in [4, 16] {
            let result = scan::scan(&factory, None, &AtomicBool::new(false), workers).unwrap();
            eprintln!(
                "profile workers={workers} digest={} metrics={:?}",
                result.digest, result.telemetry
            );
            assert_eq!(result.digest, expected);
            assert_eq!(
                result.telemetry.logical_read_bytes.load(Ordering::Relaxed),
                result.size
            );
            assert_eq!(
                result.telemetry.reader.lock().unwrap().stored_reads,
                result.size.div_ceil(native_chunk_bytes)
            );
        }
    }
}
