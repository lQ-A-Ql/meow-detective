use crate::{
    database::BatchReceiptState,
    errors::{FjallStorageError, Result},
    source_build::{MANIFEST_FILE, SEAL_FILE},
};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryState {
    Published,
    Sealed,
    MissingBatchReceipt,
    CorruptBatchReceipt,
    MissingManifest,
    MissingSeal,
    ManifestMismatch,
    PublishConflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryConsistencyReport {
    pub state: RecoveryState,
    pub build_path: PathBuf,
    pub published_path: PathBuf,
    pub batch_receipt: BatchReceiptState,
    pub manifest_digest: Option<String>,
    pub published_digest: Option<String>,
}

#[derive(Clone, Copy)]
pub struct RecoveryScan<'a> {
    pub build_path: &'a Path,
    pub published_path: &'a Path,
    pub batch_receipt: BatchReceiptState,
}

pub fn scan_recovery_consistency(scan: RecoveryScan<'_>) -> Result<RecoveryConsistencyReport> {
    let batch_receipt = scan.batch_receipt;
    let build_path = scan.build_path.to_path_buf();
    let published_path = scan.published_path.to_path_buf();
    let build_exists = scan.build_path.is_dir();
    let published_exists = scan.published_path.is_dir();

    if !build_exists && published_exists {
        let published_digest = checked_digest(scan.published_path)?;
        let state = match batch_receipt {
            BatchReceiptState::Present { .. } => RecoveryState::Published,
            BatchReceiptState::Missing => RecoveryState::MissingBatchReceipt,
            BatchReceiptState::Corrupt => RecoveryState::CorruptBatchReceipt,
        };
        return Ok(RecoveryConsistencyReport {
            state,
            build_path,
            published_path,
            batch_receipt,
            manifest_digest: Some(published_digest.clone()),
            published_digest: Some(published_digest),
        });
    }

    if !build_exists {
        return Err(FjallStorageError::InvalidBuildPath(
            scan.build_path.display().to_string(),
        ));
    }
    let manifest_digest = match checked_digest(scan.build_path) {
        Ok(digest) => digest,
        Err(FjallStorageError::MissingManifest(_)) => {
            return Ok(report(
                RecoveryState::MissingManifest,
                build_path,
                published_path,
                batch_receipt,
                None,
                None,
            ));
        }
        Err(FjallStorageError::MissingSeal(_)) => {
            return Ok(report(
                RecoveryState::MissingSeal,
                build_path,
                published_path,
                batch_receipt,
                None,
                None,
            ));
        }
        Err(FjallStorageError::ManifestDigestMismatch { .. }) => {
            return Ok(report(
                RecoveryState::ManifestMismatch,
                build_path,
                published_path,
                batch_receipt,
                None,
                None,
            ));
        }
        Err(error) => return Err(error),
    };

    if published_exists {
        let published_digest = match checked_digest(scan.published_path) {
            Ok(digest) => Some(digest),
            Err(FjallStorageError::MissingManifest(_)) => {
                return Ok(report(
                    RecoveryState::MissingManifest,
                    build_path,
                    published_path,
                    batch_receipt,
                    Some(manifest_digest),
                    None,
                ));
            }
            Err(FjallStorageError::MissingSeal(_)) => {
                return Ok(report(
                    RecoveryState::MissingSeal,
                    build_path,
                    published_path,
                    batch_receipt,
                    Some(manifest_digest),
                    None,
                ));
            }
            Err(FjallStorageError::ManifestDigestMismatch { .. }) => {
                return Ok(report(
                    RecoveryState::ManifestMismatch,
                    build_path,
                    published_path,
                    batch_receipt,
                    Some(manifest_digest),
                    None,
                ));
            }
            Err(error) => return Err(error),
        };
        if published_digest.as_deref() != Some(manifest_digest.as_str()) {
            return Ok(report(
                RecoveryState::PublishConflict,
                build_path,
                published_path,
                batch_receipt,
                Some(manifest_digest),
                published_digest,
            ));
        }
    }
    let state = match batch_receipt {
        BatchReceiptState::Present { .. } => RecoveryState::Sealed,
        BatchReceiptState::Missing => RecoveryState::MissingBatchReceipt,
        BatchReceiptState::Corrupt => RecoveryState::CorruptBatchReceipt,
    };
    Ok(report(
        state,
        build_path,
        published_path,
        batch_receipt,
        Some(manifest_digest),
        None,
    ))
}

fn report(
    state: RecoveryState,
    build_path: PathBuf,
    published_path: PathBuf,
    batch_receipt: BatchReceiptState,
    manifest_digest: Option<String>,
    published_digest: Option<String>,
) -> RecoveryConsistencyReport {
    RecoveryConsistencyReport {
        state,
        build_path,
        published_path,
        batch_receipt,
        manifest_digest,
        published_digest,
    }
}

fn checked_digest(path: &Path) -> Result<String> {
    let seal = fs::read_to_string(path.join(SEAL_FILE))
        .map_err(|error| marker_error(error, path, true))?;
    let expected = seal
        .lines()
        .nth(1)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| FjallStorageError::MissingSeal(path.to_path_buf()))?;
    let manifest = fs::read_to_string(path.join(MANIFEST_FILE))
        .map_err(|error| marker_error(error, path, false))?;
    let manifest = manifest.trim();
    if manifest.is_empty() {
        return Err(FjallStorageError::MissingManifest(path.to_path_buf()));
    }
    if manifest != expected {
        return Err(FjallStorageError::ManifestDigestMismatch {
            expected: expected.to_owned(),
            actual: manifest.to_owned(),
        });
    }
    Ok(expected.to_owned())
}

fn marker_error(error: io::Error, path: &Path, seal: bool) -> FjallStorageError {
    if error.kind() == io::ErrorKind::NotFound {
        if seal {
            FjallStorageError::MissingSeal(path.to_path_buf())
        } else {
            FjallStorageError::MissingManifest(path.to_path_buf())
        }
    } else {
        FjallStorageError::Io(error)
    }
}
