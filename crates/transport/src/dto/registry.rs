use serde::{Deserialize, Serialize};

/// A single auto-start entry from a Run / RunOnce key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryRunKeyDto {
    pub key_path: String,
    pub value_name: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// "machine" for HKLM Run/RunOnce, "user" for HKCU Run/RunOnce.
    pub scope: String,
}
