use std::sync::Arc;

use domain::structure::{ObservationDetail, ObservationIndex};
use persistence_fjall::{
    encode_key, KeyPart, KeyspaceName, PersistMode, SourceStore, VersionedEnvelope,
};
use persistence_sqlite::{DbError, DbResult};
use rusqlite::Connection;
use sha2::{Digest, Sha256};

const RECORD_KIND: &str = "structure_observation";
const KEY_FAMILY: &str = "structure.observation";

pub fn observation_detail_digest(detail: &ObservationDetail) -> DbResult<String> {
    let bytes = serde_json::to_vec(detail)
        .map_err(|error| DbError::System(format!("observation detail encoding failed: {error}")))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub fn write_observation_detail(
    conn: &Connection,
    index: &ObservationIndex,
    detail: &ObservationDetail,
) -> DbResult<()> {
    let owner = super::source_id(conn)?
        .ok_or_else(|| DbError::System("source identity is missing".into()))?;
    if owner != index.source.source_id {
        return Err(DbError::System("observation detail owner mismatch".into()));
    }
    let store = super::open_store_for_write(conn)?
        .ok_or_else(|| DbError::System("source detail storage is missing".into()))?;
    let digest = observation_detail_digest(detail)?;
    if digest != index.detail_sha256 {
        return Err(DbError::System(
            "observation detail digest does not match its index".into(),
        ));
    }
    let payload = serde_json::to_vec(detail)
        .map_err(|error| DbError::System(format!("observation detail encoding failed: {error}")))?;
    let envelope = VersionedEnvelope::new(
        RECORD_KIND,
        1,
        index.id.0.clone(),
        index.source.source_id.clone(),
        index.source.revision,
        payload,
    );
    write_envelope(&store, index, &envelope)?;
    Ok(())
}

pub fn read_observation_detail(
    conn: &Connection,
    index: &ObservationIndex,
) -> DbResult<ObservationDetail> {
    let owner = super::source_id(conn)?
        .ok_or_else(|| DbError::System("source identity is missing".into()))?;
    if owner != index.source.source_id {
        return Err(DbError::System("observation detail owner mismatch".into()));
    }
    let store = super::open_store(conn)?
        .ok_or_else(|| DbError::System("source detail storage is missing".into()))?;
    let key = observation_key(index)?;
    let envelope = store
        .read_envelope(&store.database().read_snapshot(), KeyspaceName::Meta, &key)
        .map_err(fjall_error)?
        .ok_or_else(|| DbError::System("observation detail is missing".into()))?;
    if envelope.record_kind != RECORD_KIND
        || envelope.record_id != index.id.0
        || envelope.source_id != index.source.source_id
        || envelope.revision != index.source.revision
    {
        return Err(DbError::System(
            "observation detail identity mismatch".into(),
        ));
    }
    let detail = serde_json::from_slice(&envelope.payload)
        .map_err(|error| DbError::System(format!("observation detail decoding failed: {error}")))?;
    if observation_detail_digest(&detail)? != index.detail_sha256 {
        return Err(DbError::System("observation detail digest mismatch".into()));
    }
    Ok(detail)
}

fn write_envelope(
    store: &Arc<SourceStore>,
    index: &ObservationIndex,
    envelope: &VersionedEnvelope,
) -> DbResult<()> {
    let key = observation_key(index)?;
    store
        .write_envelope(KeyspaceName::Meta, key, envelope, PersistMode::SyncAll)
        .map_err(fjall_error)?;
    Ok(())
}

fn observation_key(index: &ObservationIndex) -> DbResult<Vec<u8>> {
    encode_key(
        KEY_FAMILY,
        &[
            KeyPart::Text(index.source.source_id.clone()),
            KeyPart::U64(index.source.revision),
            KeyPart::Text(index.id.0.clone()),
        ],
    )
    .map_err(fjall_error)
}

fn fjall_error(error: persistence_fjall::FjallStorageError) -> DbError {
    DbError::System(error.to_string())
}
