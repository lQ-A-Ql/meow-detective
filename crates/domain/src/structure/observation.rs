use super::{canonical_identity, NativeVersion, SourceRevisionRef};
use serde::{Deserialize, Serialize};

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
    #[derive(Serialize)]
    struct Identity<'a> {
        source: &'a SourceRevisionRef,
        native_identity: &'a str,
        parser_version: &'a str,
        version: &'a NativeVersion,
        location: &'a ObservationLocation,
    }
    canonical_identity(
        "observation-v2",
        &Identity {
            source,
            native_identity: &observation.native_identity,
            parser_version: &observation.parser_version,
            version: &observation.version,
            location: &observation.location,
        },
    )
    .map(ObservationId)
    .map_err(|error| ObservationIndexError::Serialization(error.to_string()))
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
