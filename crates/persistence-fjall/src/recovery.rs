use crate::{
    database::BatchReceiptState,
    errors::{FjallStorageError, Result},
    source_build::{MANIFEST_FILE, SEAL_FILE},
    source_generation::{SOURCE_GENERATION_MANIFEST_FILE, SOURCE_GENERATION_SEAL_FILE},
};
use sha2::{Digest, Sha256};
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
    if !scan.build_path.is_dir() {
        return scan_without_build(scan);
    }
    scan_build(scan)
}

fn scan_without_build(scan: RecoveryScan<'_>) -> Result<RecoveryConsistencyReport> {
    if !scan.published_path.is_dir() {
        return Err(FjallStorageError::InvalidBuildPath(
            scan.build_path.display().to_string(),
        ));
    }
    let digest = checked_digest(scan.published_path)?;
    Ok(report(
        published_state(scan.batch_receipt),
        scan.build_path.to_path_buf(),
        scan.published_path.to_path_buf(),
        scan.batch_receipt,
        Some(digest.clone()),
        Some(digest),
    ))
}

fn scan_build(scan: RecoveryScan<'_>) -> Result<RecoveryConsistencyReport> {
    let build_path = scan.build_path.to_path_buf();
    let published_path = scan.published_path.to_path_buf();
    let manifest_digest = match checked_digest(scan.build_path) {
        Ok(digest) => digest,
        Err(error) => return marker_report(error, scan, None),
    };
    if !scan.published_path.is_dir() {
        return Ok(report(
            sealed_state(scan.batch_receipt),
            build_path,
            published_path,
            scan.batch_receipt,
            Some(manifest_digest),
            None,
        ));
    }
    let published_digest = match checked_digest(scan.published_path) {
        Ok(digest) => digest,
        Err(error) => return marker_report(error, scan, Some(manifest_digest)),
    };
    if published_digest != manifest_digest {
        return Ok(report(
            RecoveryState::PublishConflict,
            build_path,
            published_path,
            scan.batch_receipt,
            Some(manifest_digest),
            Some(published_digest),
        ));
    }
    Ok(report(
        sealed_state(scan.batch_receipt),
        build_path,
        published_path,
        scan.batch_receipt,
        Some(manifest_digest),
        None,
    ))
}

fn marker_report(
    error: FjallStorageError,
    scan: RecoveryScan<'_>,
    manifest_digest: Option<String>,
) -> Result<RecoveryConsistencyReport> {
    let state = match error {
        FjallStorageError::MissingManifest(_) => RecoveryState::MissingManifest,
        FjallStorageError::MissingSeal(_) => RecoveryState::MissingSeal,
        FjallStorageError::ManifestDigestMismatch { .. } => RecoveryState::ManifestMismatch,
        other => return Err(other),
    };
    Ok(report(
        state,
        scan.build_path.to_path_buf(),
        scan.published_path.to_path_buf(),
        scan.batch_receipt,
        manifest_digest,
        None,
    ))
}

fn sealed_state(receipt: BatchReceiptState) -> RecoveryState {
    match receipt {
        BatchReceiptState::Present { .. } => RecoveryState::Sealed,
        BatchReceiptState::Missing => RecoveryState::MissingBatchReceipt,
        BatchReceiptState::Corrupt => RecoveryState::CorruptBatchReceipt,
    }
}

fn published_state(receipt: BatchReceiptState) -> RecoveryState {
    match receipt {
        BatchReceiptState::Present { .. } => RecoveryState::Published,
        BatchReceiptState::Missing => RecoveryState::MissingBatchReceipt,
        BatchReceiptState::Corrupt => RecoveryState::CorruptBatchReceipt,
    }
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
    if path.join(SOURCE_GENERATION_MANIFEST_FILE).is_file()
        || path.join(SOURCE_GENERATION_SEAL_FILE).is_file()
    {
        return checked_source_generation_digest(path);
    }
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

fn checked_source_generation_digest(path: &Path) -> Result<String> {
    let manifest_path = path.join(SOURCE_GENERATION_MANIFEST_FILE);
    let bytes = fs::read(&manifest_path).map_err(|error| marker_error(error, path, false))?;
    let digest = hex::encode(Sha256::digest(&bytes));
    let seal = fs::read_to_string(path.join(SOURCE_GENERATION_SEAL_FILE))
        .map_err(|error| source_generation_seal_error(error, path))?;
    let mut lines = seal.lines();
    if lines.next() != Some("source-generation-v1") {
        return Err(FjallStorageError::MissingSeal(path.to_path_buf()));
    }
    let expected = lines
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| FjallStorageError::MissingSeal(path.to_path_buf()))?;
    if expected != digest {
        return Err(FjallStorageError::ManifestDigestMismatch {
            expected: expected.to_owned(),
            actual: digest,
        });
    }
    Ok(expected.to_owned())
}

fn source_generation_seal_error(error: io::Error, path: &Path) -> FjallStorageError {
    if error.kind() == io::ErrorKind::NotFound {
        FjallStorageError::MissingSeal(path.to_path_buf())
    } else {
        FjallStorageError::Io(error)
    }
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
