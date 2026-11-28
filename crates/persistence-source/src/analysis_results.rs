mod source;
mod storage;

use std::path::Path;

use domain::structure::{SnapshotAnalysisReceipt, SourceRevisionRef};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use source::AnalysisSource;

const RECEIPT_PREFIX: &str = "snapshot-analysis-receipt-v1-";
const DIRECTORY_NAME: &str = "analysis-results";
const RECORD_KIND: &str = "snapshot_analysis_result";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisResultRef {
    pub source_id: String,
    pub receipt_id: String,
    pub output_sha256: String,
    pub manifest_sha256: String,
    pub generation_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisResult {
    pub reference: AnalysisResultRef,
    pub receipt: SnapshotAnalysisReceipt,
    pub output: Value,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredPayload {
    receipt: SnapshotAnalysisReceipt,
    output: Value,
}

pub fn write_analysis_result(
    case_root: &Path,
    receipt: &SnapshotAnalysisReceipt,
    output: &Value,
) -> Result<AnalysisResultRef, AnalysisResultError> {
    receipt.validate()?;
    validate_receipt_id(&receipt.id)?;
    verify_output(receipt, output)?;
    let source = AnalysisSource::acquire(case_root, &receipt.source_revision)?;
    source.require_receipt_inputs(receipt)?;
    let (directory, created) = source.result_directory(&receipt.id, true)?;
    let result = if created {
        storage::write(&directory, receipt, output)?
    } else {
        let result = storage::read(&directory, &receipt.source_revision, &receipt.id)?;
        if result.receipt != *receipt || result.output != *output {
            return Err(AnalysisResultError::ContentConflict);
        }
        result.reference
    };
    source.require_receipt_inputs(receipt)?;
    Ok(result)
}

pub fn read_analysis_result(
    case_root: &Path,
    source: &SourceRevisionRef,
    receipt_id: &str,
) -> Result<AnalysisResult, AnalysisResultError> {
    validate_receipt_id(receipt_id)?;
    let source = AnalysisSource::acquire(case_root, source)?;
    let (directory, _) = source.result_directory(receipt_id, false)?;
    let result = storage::read(&directory, source.revision(), receipt_id)?;
    source.require_receipt_inputs(&result.receipt)?;
    Ok(result)
}

#[derive(Debug, thiserror::Error)]
pub enum AnalysisResultError {
    #[error("analysis result receipt identity is unsafe")]
    InvalidReceiptId,
    #[error("analysis result directory is outside its source or is a link")]
    UnsafePath,
    #[error("analysis result source ownership does not match")]
    SourceOwnerMismatch,
    #[error("analysis result source dependency revisions do not match")]
    DependencyMismatch,
    #[error("analysis result input source has changed")]
    SourceChanged,
    #[error("analysis result input generation does not match its receipt")]
    GenerationMismatch,
    #[error("analysis result output digest does not match its receipt")]
    OutputDigestMismatch,
    #[error("analysis result envelope identity does not match")]
    EnvelopeMismatch,
    #[error("analysis result manifest does not match its payload or receipt")]
    ManifestMismatch,
    #[error("analysis result conflicts with immutable existing contents")]
    ContentConflict,
    #[error(transparent)]
    Receipt(#[from] domain::structure::SnapshotAnalysisReceiptError),
    #[error(transparent)]
    Database(#[from] persistence_sqlite::DbError),
    #[error(transparent)]
    Storage(#[from] crate::fjall::FjallStorageError),
    #[error(transparent)]
    Runtime(#[from] crate::runtime::StorageRuntimeError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn validate_receipt_id(value: &str) -> Result<(), AnalysisResultError> {
    if !value.strip_prefix(RECEIPT_PREFIX).is_some_and(valid_sha256) {
        return Err(AnalysisResultError::InvalidReceiptId);
    }
    Ok(())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn verify_output(
    receipt: &SnapshotAnalysisReceipt,
    output: &Value,
) -> Result<(), AnalysisResultError> {
    if sha256(&serde_json::to_vec(output)?) != receipt.output_sha256 {
        return Err(AnalysisResultError::OutputDigestMismatch);
    }
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
