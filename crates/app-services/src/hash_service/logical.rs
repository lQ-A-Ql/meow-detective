use super::{
    digest_reader_exact, e01_pipeline, io_error, EvidenceHashError, EvidenceHashResult, HashService,
};
use domain::DataSourceKind;
use evidence_core::EvidenceReader;
use infrastructure::hashing;
use std::path::Path;
use std::sync::atomic::AtomicBool;

impl HashService {
    /// Hashes the logical byte stream exposed by a source reader. This is
    /// intentionally separate from `hash_evidence_with_algorithm`, whose
    /// container-set result covers the physical files that make up an image
    /// (for example a QCOW backing chain or VMDK descriptor plus extent).
    pub fn hash_logical_evidence(
        path: &Path,
        kind: &DataSourceKind,
        algorithm: hashing::HashAlgorithm,
        cancelled: &AtomicBool,
        progress: &(dyn Fn(u64, u64) + Sync),
    ) -> Result<EvidenceHashResult, EvidenceHashError> {
        if *kind == DataSourceKind::LocalDisk {
            return hash_local_disk(path, algorithm, cancelled, progress);
        }
        if *kind == DataSourceKind::E01 {
            let reader = image_e01::E01Reader::open(path)
                .map_err(|error| io_error("open logical E01 image", error))?;
            let total_bytes = reader.info().size;
            let result =
                e01_pipeline::digest_logical(reader, total_bytes, algorithm, cancelled, progress)
                    .map_err(|error| io_error("read logical E01 image", error))?
                    .ok_or(EvidenceHashError::Cancelled)?;
            return Ok(EvidenceHashResult {
                digest: result.digest,
                bytes_processed: total_bytes,
                acceleration: acceleration(algorithm),
                parallel_segments: 1,
                worker_threads: result.worker_threads,
            });
        }
        let mut reader: Box<dyn EvidenceReader> = match kind {
            DataSourceKind::E01 => unreachable!("E01 takes the bounded parallel path"),
            DataSourceKind::Raw => Box::new(
                evidence_core::RawImageReader::open(path)
                    .map_err(|error| io_error("open logical raw image", error))?,
            ),
            DataSourceKind::AndroidSparse => Box::new(
                image_android::AndroidSparseReader::open(path).map_err(|error| {
                    io_error("open logical Android image", std::io::Error::other(error))
                })?,
            ),
            _ => return Err(EvidenceHashError::Unsupported),
        };
        let total_bytes = reader.info().size;
        let digest = digest_reader_exact(
            reader.as_mut(),
            algorithm,
            total_bytes,
            cancelled,
            |processed| progress(processed.min(total_bytes), total_bytes),
        )?;
        Ok(EvidenceHashResult {
            digest,
            bytes_processed: total_bytes,
            acceleration: acceleration(algorithm),
            parallel_segments: 1,
            worker_threads: 1,
        })
    }
}

pub(super) fn hash_local_disk(
    path: &Path,
    algorithm: hashing::HashAlgorithm,
    cancelled: &AtomicBool,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<EvidenceHashResult, EvidenceHashError> {
    let mut reader = evidence_core::LocalDiskReader::open(path)
        .map_err(|error| io_error("open local physical disk", error))?;
    let total_bytes = reader.len();
    let digest = digest_reader_exact(
        &mut reader,
        algorithm,
        total_bytes,
        cancelled,
        |processed| progress(processed.min(total_bytes), total_bytes),
    )?;
    Ok(EvidenceHashResult {
        digest,
        bytes_processed: total_bytes,
        acceleration: acceleration(algorithm),
        parallel_segments: 1,
        worker_threads: 1,
    })
}

fn acceleration(algorithm: hashing::HashAlgorithm) -> &'static str {
    if algorithm == hashing::HashAlgorithm::Sha256 {
        HashService::sha256_acceleration()
    } else {
        "portable"
    }
}
