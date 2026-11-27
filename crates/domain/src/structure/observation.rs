use serde::{Deserialize, Serialize};

use super::NativeVersion;

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
