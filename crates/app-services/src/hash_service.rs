//! Hash calculation services.

use domain::DataSourceKind;
use infrastructure::hashing;
use rayon::prelude::{IndexedParallelIterator, IntoParallelRefIterator, ParallelIterator};
use std::io::{self, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub(crate) mod volumes;

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
    #[error("directory evidence exceeds the 1,000,000 entry hash limit")]
    DirectoryEntryLimit,
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
    pub fn digest_reader(
        reader: &mut dyn Read,
        algorithm: hashing::HashAlgorithm,
        cancelled: impl Fn() -> bool,
        progress: impl FnMut(u64),
    ) -> io::Result<Option<String>> {
        hashing::digest_reader_with_cancel(reader, algorithm, cancelled, progress)
    }

    pub fn digest_file(path: &Path, algorithm: hashing::HashAlgorithm) -> io::Result<String> {
        let mut file = std::fs::File::open(path)?;
        hashing::digest_reader_with_cancel(&mut file, algorithm, || false, |_| {}).and_then(
            |digest| {
                digest.ok_or_else(|| io::Error::new(io::ErrorKind::Interrupted, "digest cancelled"))
            },
        )
    }

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
        Self::hash_evidence_with_algorithm(
            path,
            kind,
            hashing::HashAlgorithm::Sha256,
            cancelled,
            progress,
        )
    }

    pub fn hash_evidence_with_algorithm(
        path: &Path,
        kind: &DataSourceKind,
        algorithm: hashing::HashAlgorithm,
        cancelled: &AtomicBool,
        progress: &(dyn Fn(u64, u64) + Sync),
    ) -> Result<EvidenceHashResult, EvidenceHashError> {
        if *kind == DataSourceKind::LocalDisk {
            return hash_local_disk(path, algorithm, cancelled, progress);
        }
        let segments = match kind {
            DataSourceKind::E01 => volumes::discover_e01_segments(path)
                .map_err(|error| io_error("discover E01 segments", error))?,
            DataSourceKind::Raw => evidence_core::RawImageReader::open(path)
                .map(|reader| reader.backing_paths().to_vec())
                .map_err(|error| io_error("discover raw image backing files", error))?,
            DataSourceKind::AndroidSparse => vec![path.to_path_buf()],
            DataSourceKind::LogicalArchive => vec![path.to_path_buf()],
            DataSourceKind::LogicalDirectory | DataSourceKind::CephRbd | DataSourceKind::CephFs => {
                return hash_directory_evidence(path, algorithm, cancelled, progress)
            }
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
        let context = SegmentHashContext {
            algorithm,
            total_bytes,
            pipelined,
            cancelled,
            processed: &processed,
            progress,
        };
        let digests = segments
            .par_iter()
            .enumerate()
            .map(|(index, segment)| hash_segment(index, segment, &context))
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
                    "segment={:08};length={};{}={}",
                    segment.index,
                    segment.length,
                    algorithm.label(),
                    segment.digest
                );
            }
            hashing::digest_bytes(manifest.as_bytes(), algorithm)
        };
        Ok(EvidenceHashResult {
            digest,
            bytes_processed: total_bytes,
            acceleration: if algorithm == hashing::HashAlgorithm::Sha256 {
                Self::sha256_acceleration()
            } else {
                "portable"
            },
            parallel_segments: digests.len(),
            worker_threads: if pipelined && algorithm == hashing::HashAlgorithm::Sha256 {
                hashing::sha256_pipeline_worker_threads()
            } else {
                digests.len().min(rayon::current_num_threads()).max(1)
            },
        })
    }

    /// Hashes the logical byte stream exposed by a source reader.  This is
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
        let mut reader: Box<dyn evidence_core::EvidenceReader> = match kind {
            DataSourceKind::E01 => Box::new(
                image_e01::E01Reader::open(path)
                    .map_err(|error| io_error("open logical E01 image", error))?,
            ),
            DataSourceKind::Raw => Box::new(
                evidence_core::RawImageReader::open(path)
                    .map_err(|error| io_error("open logical raw image", error))?,
            ),
            DataSourceKind::AndroidSparse => Box::new(
                image_android::AndroidSparseReader::open(path).map_err(|error| {
                    io_error("open logical Android image", io::Error::other(error))
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
            acceleration: if algorithm == hashing::HashAlgorithm::Sha256 {
                HashService::sha256_acceleration()
            } else {
                "portable"
            },
            parallel_segments: 1,
            worker_threads: 1,
        })
    }
}

fn hash_local_disk(
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
        acceleration: if algorithm == hashing::HashAlgorithm::Sha256 {
            HashService::sha256_acceleration()
        } else {
            "portable"
        },
        parallel_segments: 1,
        worker_threads: 1,
    })
}

fn hash_directory_evidence(
    root: &Path,
    algorithm: hashing::HashAlgorithm,
    cancelled: &AtomicBool,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<EvidenceHashResult, EvidenceHashError> {
    let mut entries = Vec::new();
    collect_directory_entries(root, root, cancelled, &mut entries)?;
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    let total_bytes =
        entries
            .iter()
            .filter_map(|entry| entry.2)
            .try_fold(0u64, |total, length| {
                total.checked_add(length).ok_or(EvidenceHashError::Io {
                    operation: "sum directory evidence length",
                    kind: io::ErrorKind::InvalidData,
                })
            })?;
    let mut processed = 0u64;
    let mut manifest = String::new();
    for (relative, kind, length, path) in &entries {
        if cancelled.load(Ordering::Acquire) {
            return Err(EvidenceHashError::Cancelled);
        }
        match (*kind, *length) {
            ('f', Some(length)) => {
                let mut file = std::fs::File::open(path)
                    .map_err(|error| io_error("open directory evidence file", error))?;
                let digest =
                    digest_reader_exact(&mut file, algorithm, length, cancelled, |local| {
                        let completed = processed.saturating_add(local).min(total_bytes);
                        (progress)(completed, total_bytes);
                    })?;
                processed = processed.saturating_add(length);
                use std::fmt::Write;
                let _ = writeln!(manifest, "f\t{relative}\t{length}\t{digest}");
            }
            ('d', _) => {
                use std::fmt::Write;
                let _ = writeln!(manifest, "d\t{relative}");
            }
            ('s', _) => {
                use std::fmt::Write;
                let _ = writeln!(manifest, "s\t{relative}");
            }
            _ => {
                return Err(EvidenceHashError::Io {
                    operation: "classify directory evidence entry",
                    kind: io::ErrorKind::InvalidData,
                })
            }
        }
    }
    if cancelled.load(Ordering::Acquire) {
        return Err(EvidenceHashError::Cancelled);
    }
    (progress)(total_bytes, total_bytes);
    Ok(EvidenceHashResult {
        digest: hashing::digest_bytes(manifest.as_bytes(), algorithm),
        bytes_processed: total_bytes,
        acceleration: if algorithm == hashing::HashAlgorithm::Sha256 {
            HashService::sha256_acceleration()
        } else {
            "portable"
        },
        parallel_segments: entries.len().max(1),
        worker_threads: 1,
    })
}

fn collect_directory_entries(
    root: &Path,
    current: &Path,
    cancelled: &AtomicBool,
    entries: &mut Vec<(String, char, Option<u64>, std::path::PathBuf)>,
) -> Result<(), EvidenceHashError> {
    let mut pending = vec![current.to_path_buf()];
    while let Some(directory) = pending.pop() {
        if cancelled.load(Ordering::Acquire) {
            return Err(EvidenceHashError::Cancelled);
        }
        for item in std::fs::read_dir(&directory)
            .map_err(|error| io_error("read directory evidence", error))?
        {
            if cancelled.load(Ordering::Acquire) {
                return Err(EvidenceHashError::Cancelled);
            }
            let item = item.map_err(|error| io_error("enumerate directory evidence", error))?;
            let path = item.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|_| EvidenceHashError::Io {
                    operation: "normalize directory evidence path",
                    kind: io::ErrorKind::InvalidData,
                })?
                .to_string_lossy()
                .replace('\\', "/");
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|error| io_error("inspect directory evidence entry", error))?;
            let file_type = metadata.file_type();
            let kind = if file_type.is_symlink() {
                's'
            } else if file_type.is_dir() {
                pending.push(path.clone());
                'd'
            } else if file_type.is_file() {
                'f'
            } else {
                's'
            };
            entries.push((
                relative,
                kind,
                (kind == 'f').then_some(metadata.len()),
                path,
            ));
            check_directory_entry_limit(entries.len())?;
        }
    }
    Ok(())
}

fn check_directory_entry_limit(count: usize) -> Result<(), EvidenceHashError> {
    if count > 1_000_000 {
        Err(EvidenceHashError::DirectoryEntryLimit)
    } else {
        Ok(())
    }
}

#[derive(Debug)]
struct SegmentDigest {
    index: usize,
    length: u64,
    digest: String,
}

struct SegmentHashContext<'a> {
    algorithm: hashing::HashAlgorithm,
    total_bytes: u64,
    pipelined: bool,
    cancelled: &'a AtomicBool,
    processed: &'a AtomicU64,
    progress: &'a (dyn Fn(u64, u64) + Sync),
}

fn hash_segment(
    index: usize,
    path: &Path,
    context: &SegmentHashContext<'_>,
) -> Result<SegmentDigest, EvidenceHashError> {
    let length = std::fs::metadata(path)
        .map_err(|error| io_error("inspect evidence", error))?
        .len();
    let mut previous = 0u64;
    let mut local_processed = 0u64;
    let mut track_progress = |local: u64| {
        let delta = local.saturating_sub(previous);
        previous = local;
        local_processed = local;
        let completed = context
            .processed
            .fetch_add(delta, Ordering::AcqRel)
            .saturating_add(delta);
        (context.progress)(completed.min(context.total_bytes), context.total_bytes);
    };
    let digest = if context.pipelined && context.algorithm == hashing::HashAlgorithm::Sha256 {
        hashing::sha256_file_pipelined_with_cancel(
            path,
            &|| context.cancelled.load(Ordering::Acquire),
            &mut track_progress,
        )
    } else {
        let mut file =
            std::fs::File::open(path).map_err(|error| io_error("open evidence", error))?;
        hashing::digest_reader_with_cancel(
            &mut file,
            context.algorithm,
            || context.cancelled.load(Ordering::Acquire),
            &mut track_progress,
        )
    }
    .map_err(|error| io_error("read evidence", error))?
    .ok_or(EvidenceHashError::Cancelled)?;
    if local_processed != length {
        return Err(short_read_error("read complete evidence segment"));
    }
    Ok(SegmentDigest {
        index,
        length,
        digest,
    })
}

fn digest_reader_exact(
    reader: &mut dyn Read,
    algorithm: hashing::HashAlgorithm,
    expected_bytes: u64,
    cancelled: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> Result<String, EvidenceHashError> {
    let mut processed = 0u64;
    let digest = hashing::digest_reader_with_cancel(
        reader,
        algorithm,
        || cancelled.load(Ordering::Acquire),
        |amount| {
            processed = amount;
            progress(amount);
        },
    )
    .map_err(|error| io_error("read complete evidence input", error))?
    .ok_or(EvidenceHashError::Cancelled)?;
    if processed != expected_bytes {
        return Err(short_read_error("read complete evidence input"));
    }
    Ok(digest)
}

fn short_read_error(operation: &'static str) -> EvidenceHashError {
    EvidenceHashError::Io {
        operation,
        kind: io::ErrorKind::UnexpectedEof,
    }
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
