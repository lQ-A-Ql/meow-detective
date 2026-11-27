use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{NativeVersion, SourceRevisionRef};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObservationId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObservationLocation {
    /// A verified source window containing the fact, not its exact encoded extent.
    SourceWindow {
        byte_offset: u64,
        byte_length: u64,
    },
    Unavailable {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeObservation {
    pub native_identity: String,
    pub version: NativeVersion,
    pub location: ObservationLocation,
    pub parser_version: String,
    pub native_checksum_verified: bool,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservationIndex {
    pub id: ObservationId,
    pub source: SourceRevisionRef,
    pub native_identity: String,
    pub version: NativeVersion,
    pub location: ObservationLocation,
    pub parser_version: String,
    pub native_checksum_verified: bool,
    pub detail_key: String,
    pub detail_sha256: String,
    pub attempt_id: String,
    pub generation: String,
    pub receipt: String,
}

impl ObservationIndex {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source: SourceRevisionRef,
        observation: &NativeObservation,
        detail_sha256: String,
        attempt_id: String,
        generation: String,
        receipt: String,
    ) -> Result<Self, ObservationIndexError> {
        if observation.native_identity.is_empty()
            || observation.parser_version.is_empty()
            || attempt_id.is_empty()
            || generation.is_empty()
            || !valid_digest(&detail_sha256)
            || !valid_digest(&receipt)
        {
            return Err(ObservationIndexError::InvalidIdentity);
        }
        let id = observation_id(&source, observation)?;
        Ok(Self {
            detail_key: format!("structure.observation.{}", id.0),
            id,
            source,
            native_identity: observation.native_identity.clone(),
            version: observation.version.clone(),
            location: observation.location.clone(),
            parser_version: observation.parser_version.clone(),
            native_checksum_verified: observation.native_checksum_verified,
            detail_sha256,
            attempt_id,
            generation,
            receipt,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservationDetail {
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ObservationIndexError {
    #[error("observation index identity is invalid")]
    InvalidIdentity,
    #[error("observation index serialization failed: {0}")]
    Serialization(String),
}

fn observation_id(
    source: &SourceRevisionRef,
    observation: &NativeObservation,
) -> Result<ObservationId, ObservationIndexError> {
    let version = serde_json::to_vec(&observation.version)
        .map_err(|error| ObservationIndexError::Serialization(error.to_string()))?;
    let location = serde_json::to_vec(&observation.location)
        .map_err(|error| ObservationIndexError::Serialization(error.to_string()))?;
    let mut digest = Sha256::new();
    digest.update(b"observation-v1");
    for value in [
        source.case_id.as_bytes(),
        source.source_id.as_bytes(),
        observation.native_identity.as_bytes(),
        observation.parser_version.as_bytes(),
        version.as_slice(),
        location.as_slice(),
    ] {
        digest.update((value.len() as u64).to_le_bytes());
        digest.update(value);
    }
    digest.update(source.revision.to_le_bytes());
    Ok(ObservationId(format!(
        "observation-v1-{}",
        hex::encode(digest.finalize())
    )))
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
