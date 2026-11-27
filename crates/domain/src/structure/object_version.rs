use serde::{Deserialize, Serialize};

use super::NativeVersion;

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
