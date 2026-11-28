use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::SourceRevisionRef;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalContentBinding {
    pub source: SourceRevisionRef,
    pub logical_size: u64,
    pub logical_sha256: String,
    pub manifest_sha256: String,
    pub backing_paths: Vec<PathBuf>,
}
