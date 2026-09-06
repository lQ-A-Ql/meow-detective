//! Hash calculation services.

use domain::DataSourceKind;
use infrastructure::hashing;
use rayon::prelude::{IndexedParallelIterator, IntoParallelRefIterator, ParallelIterator};
use std::io::{self, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub mod evidence_jobs;
mod volumes;

#[derive(Debug, thiserror::Error)]
pub enum EvidenceHashError {
    #[error("evidence hash I/O failed during {operation} ({kind:?})")]
    Io {
        operation: &'static str,
        kind: io::ErrorKind,
    },
    #[error("evidence kind cannot be hashed")]
    Unsupported,
    #[error("evidence hash cancelled")]
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceHashResult {
    pub digest: String,
    pub bytes_processed: u64,
    pub acceleration: &'static str,
    pub parallel_segments: usize,
    pub worker_threads: usize,
}

pub struct HashService;

impl HashService {
    pub fn sha256_reader(reader: &mut dyn Read) -> io::Result<String> {
        hashing::sha256_reader(reader)
    }

    pub fn sha256_file(path: &Path) -> io::Result<String> {
        hashing::sha256_file(path)
    }

    pub fn sha256_bytes(data: &[u8]) -> String {
        hashing::sha256_bytes(data)
    }

    pub fn verify_sha256(data: &[u8], expected_hash: &str) -> bool {
        hashing::verify_sha256(data, expected_hash)
    }

    pub fn sha256_acceleration() -> &'static str {
        hashing::sha256_acceleration().label()
    }

    pub fn hash_evidence(
        path: &Path,
        kind: &DataSourceKind,
        cancelled: &AtomicBool,
        progress: &(dyn Fn(u64, u64) + Sync),
    ) -> Result<EvidenceHashResult, EvidenceHashError> {
        if *kind == DataSourceKind::LocalDisk {
            return hash_local_disk(path, cancelled, progress);
        }
        let segments = match kind {
            DataSourceKind::E01 => volumes::discover_e01_segments(path)
                .map_err(|error| io_error("discover E01 segments", error))?,
            DataSourceKind::Raw => evidence_core::RawImageReader::open(path)
                .map(|reader| reader.backing_paths().to_vec())
                .map_err(|error| io_error("discover raw image backing files", error))?,
            _ => return Err(EvidenceHashError::Unsupported),
        };
        let total_bytes = segments
            .iter()
            .map(|segment| {
                std::fs::metadata(segment)
                    .map(|metadata| metadata.len())
                    .map_err(|error| io_error("inspect evidence segment", error))
            })
            .try_fold(0u64, |total, length| {
                length.and_then(|value| {
                    total.checked_add(value).ok_or(EvidenceHashError::Io {
                        operation: "sum evidence length",
                        kind: io::ErrorKind::InvalidData,
                    })
                })
            })?;
        let processed = AtomicU64::new(0);
        let pipelined = segments.len() == 1;
        let digests = segments
            .par_iter()
            .enumerate()
            .map(|(index, segment)| {
                hash_segment(
                    index,
                    segment,
                    total_bytes,
                    pipelined,
                    cancelled,
                    &processed,
                    progress,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        if cancelled.load(Ordering::Acquire) {
            return Err(EvidenceHashError::Cancelled);
        }
        let digest = if digests.len() == 1 {
            digests[0].digest.clone()
        } else {
            let mut manifest = String::new();
            for segment in &digests {
                use std::fmt::Write;
                let _ = writeln!(
                    manifest,
                    "segment={:08};length={};sha256={}",
                    segment.index, segment.length, segment.digest
                );
            }
            hashing::sha256_bytes(manifest.as_bytes())
        };
        Ok(EvidenceHashResult {
            digest,
            bytes_processed: total_bytes,
            acceleration: Self::sha256_acceleration(),
            parallel_segments: digests.len(),
            worker_threads: if pipelined {
                hashing::sha256_pipeline_worker_threads()
            } else {
                digests.len().min(rayon::current_num_threads()).max(1)
            },
        })
    }
}

fn hash_local_disk(
    path: &Path,
    cancelled: &AtomicBool,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<EvidenceHashResult, EvidenceHashError> {
    let mut reader = evidence_core::LocalDiskReader::open(path)
        .map_err(|error| io_error("open local physical disk", error))?;
    let total_bytes = reader.len();
    let digest = hashing::sha256_reader_with_cancel(
        &mut reader,
        || cancelled.load(Ordering::Acquire),
        |processed| progress(processed.min(total_bytes), total_bytes),
    )
    .map_err(|error| io_error("read local physical disk", error))?
    .ok_or(EvidenceHashError::Cancelled)?;
    Ok(EvidenceHashResult {
        digest,
        bytes_processed: total_bytes,
        acceleration: HashService::sha256_acceleration(),
        parallel_segments: 1,
        worker_threads: 1,
    })
}

#[derive(Debug)]
struct SegmentDigest {
    index: usize,
    length: u64,
    digest: String,
}

fn hash_segment(
    index: usize,
    path: &Path,
    total_bytes: u64,
    pipelined: bool,
    cancelled: &AtomicBool,
    processed: &AtomicU64,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<SegmentDigest, EvidenceHashError> {
    let length = std::fs::metadata(path)
        .map_err(|error| io_error("inspect evidence", error))?
        .len();
    let previous = AtomicU64::new(0);
    let mut track_progress = |local: u64| {
        let delta = local.saturating_sub(previous.swap(local, Ordering::AcqRel));
        let completed = processed
            .fetch_add(delta, Ordering::AcqRel)
            .saturating_add(delta);
        progress(completed.min(total_bytes), total_bytes);
    };
    let digest = if pipelined {
        hashing::sha256_file_pipelined_with_cancel(
            path,
            &|| cancelled.load(Ordering::Acquire),
            &mut track_progress,
        )
    } else {
        let mut file =
            std::fs::File::open(path).map_err(|error| io_error("open evidence", error))?;
        hashing::sha256_reader_with_cancel(
            &mut file,
            || cancelled.load(Ordering::Acquire),
            &mut track_progress,
        )
    }
    .map_err(|error| io_error("read evidence", error))?
    .ok_or(EvidenceHashError::Cancelled)?;
    Ok(SegmentDigest {
        index,
        length,
        digest,
    })
}

fn io_error(operation: &'static str, error: io::Error) -> EvidenceHashError {
    EvidenceHashError::Io {
        operation,
        kind: error.kind(),
    }
}

#[cfg(test)]
#[path = "../tests/unit/hash_service.rs"]
mod tests;
