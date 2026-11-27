use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRevisionRef {
    pub case_id: String,
    pub source_id: String,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRevisionContent {
    pub logical_size: u64,
    pub metadata_identity: Option<String>,
    pub logical_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NativeVersion {
    LvmLabel,
    LvmCommitted {
        vg_uuid: String,
        seqno: u64,
    },
    Md {
        metadata_version: String,
        events: u64,
    },
}

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
pub struct StorageObjectVersion {
    pub id: String,
    pub storage_object_id: String,
    pub derived_source_id: String,
    pub adapter_version: String,
    pub mapping_sha256: String,
    pub native_versions: Vec<NativeVersion>,
}
