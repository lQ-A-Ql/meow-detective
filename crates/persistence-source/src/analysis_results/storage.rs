use std::{path::Path, sync::atomic::AtomicBool};

use domain::structure::{SnapshotAnalysisReceipt, SourceRevisionRef};
use serde_json::Value;

use crate::fjall::{
    encode_key, seal_source_generation_with_control, source_generation_manifest_digest,
    DatabaseConfig, KeyPart, KeyspaceName, PersistMode, VersionedEnvelope,
};
use crate::runtime::{
    ReadOnlySourceStorageHandle, ReadOnlyStorageRuntime, ReadWriteStorageRuntime,
};

use super::{
    sha256, verify_output, AnalysisResult, AnalysisResultError, AnalysisResultRef, StoredPayload,
    DIRECTORY_NAME, RECORD_KIND,
};

pub(super) fn write(
    directory: &Path,
    receipt: &SnapshotAnalysisReceipt,
    output: &Value,
) -> Result<AnalysisResultRef, AnalysisResultError> {
    let payload = serde_json::to_vec(&StoredPayload {
        receipt: receipt.clone(),
        output: output.clone(),
    })?;
    let payload_sha256 = sha256(&payload);
    let runtime = ReadWriteStorageRuntime::open_with_config(directory, DatabaseConfig::default())?;
    let store = runtime.source();
    let envelope = VersionedEnvelope::new(
        RECORD_KIND,
        1,
        &receipt.id,
        &receipt.source_revision.source_id,
        receipt.source_revision.revision,
        payload,
    );
    store.write_envelope(
        KeyspaceName::Meta,
        key(&receipt.id)?,
        &envelope,
        PersistMode::SyncAll,
    )?;
    seal_source_generation_with_control(
        store.source_store(),
        &receipt.source_revision.source_id,
        &receipt.attempt.attempt_id,
        &payload_sha256,
        &AtomicBool::new(false),
        |_, _| {},
    )?;
    Ok(read(directory, &receipt.source_revision, &receipt.id)?.reference)
}

pub(super) fn read(
    directory: &Path,
    source: &SourceRevisionRef,
    receipt_id: &str,
) -> Result<AnalysisResult, AnalysisResultError> {
    source_generation_manifest_digest(directory)?;
    let runtime = ReadOnlyStorageRuntime::open(directory)?;
    read_store(&runtime.source(), source, receipt_id)
}

fn read_store(
    store: &ReadOnlySourceStorageHandle,
    source: &SourceRevisionRef,
    receipt_id: &str,
) -> Result<AnalysisResult, AnalysisResultError> {
    let manifest = store.verify_source_generation_with_control(
        &source.source_id,
        &AtomicBool::new(false),
        |_, _| {},
    )?;
    let snapshot = store.snapshot();
    let envelope = store
        .read_envelope(&snapshot, KeyspaceName::Meta, &key(receipt_id)?)?
        .ok_or(AnalysisResultError::EnvelopeMismatch)?;
    if envelope.record_kind != RECORD_KIND
        || envelope.schema_version != 1
        || envelope.record_id != receipt_id
        || envelope.source_id != source.source_id
        || envelope.revision != source.revision
    {
        return Err(AnalysisResultError::EnvelopeMismatch);
    }
    let StoredPayload { receipt, output } = serde_json::from_slice(&envelope.payload)?;
    receipt.validate()?;
    if receipt.id != receipt_id || receipt.source_revision != *source {
        return Err(AnalysisResultError::EnvelopeMismatch);
    }
    verify_output(&receipt, &output)?;
    if manifest.attempt_id != receipt.attempt.attempt_id
        || manifest.projection_batch_digest != sha256(&envelope.payload)
    {
        return Err(AnalysisResultError::ManifestMismatch);
    }
    Ok(AnalysisResult {
        reference: AnalysisResultRef {
            source_id: source.source_id.clone(),
            receipt_id: receipt_id.into(),
            output_sha256: receipt.output_sha256.clone(),
            manifest_sha256: source_generation_manifest_digest(store.source_root())?,
            generation_name: format!("{DIRECTORY_NAME}/{receipt_id}"),
        },
        receipt,
        output,
    })
}

fn key(receipt_id: &str) -> Result<Vec<u8>, AnalysisResultError> {
    Ok(encode_key(
        RECORD_KIND,
        &[KeyPart::Text(receipt_id.into())],
    )?)
}
