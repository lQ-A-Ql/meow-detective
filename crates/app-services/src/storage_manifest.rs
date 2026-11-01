use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const STORAGE_MANIFEST_FILE: &str = "storage-manifest.json";
pub const STORAGE_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum StorageManifestError {
    #[error("storage manifest I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage manifest encoding failed: {0}")]
    Encoding(#[from] serde_json::Error),
    #[error("storage manifest is missing: {0}")]
    Missing(PathBuf),
    #[error("storage manifest format is unsupported: {0}")]
    Unsupported(u32),
    #[error("storage manifest case identity does not match: {0}")]
    CaseIdentity(String),
    #[error("storage manifest model is unsupported: {0}")]
    Model(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StorageManifest {
    pub format_version: u32,
    pub model: String,
    pub case_id: String,
    pub control_database: String,
    pub source_directory: String,
    pub artifact_directory: String,
}

pub fn initialize_case(
    root: &Path,
    case_id: &str,
) -> Result<StorageManifest, StorageManifestError> {
    std::fs::create_dir_all(root.join("sources"))?;
    std::fs::create_dir_all(root.join("artifacts").join("objects"))?;
    std::fs::create_dir_all(root.join("fjall"))?;
    let manifest = StorageManifest {
        format_version: STORAGE_FORMAT_VERSION,
        model: "sqlite-fjall-v1".to_owned(),
        case_id: case_id.to_owned(),
        control_database: "app.db".to_owned(),
        source_directory: "sources".to_owned(),
        artifact_directory: "artifacts/objects".to_owned(),
    };
    let path = root.join(STORAGE_MANIFEST_FILE);
    let bytes = serde_json::to_vec_pretty(&manifest)?;
    std::fs::write(path, bytes)?;
    Ok(manifest)
}

pub fn validate_case(root: &Path, case_id: &str) -> Result<StorageManifest, StorageManifestError> {
    let path = root.join(STORAGE_MANIFEST_FILE);
    if !path.is_file() {
        return Err(StorageManifestError::Missing(path));
    }
    let manifest: StorageManifest = serde_json::from_slice(&std::fs::read(path)?)?;
    if manifest.format_version != STORAGE_FORMAT_VERSION {
        return Err(StorageManifestError::Unsupported(manifest.format_version));
    }
    if manifest.model != "sqlite-fjall-v1" {
        return Err(StorageManifestError::Model(manifest.model));
    }
    if manifest.case_id != case_id {
        return Err(StorageManifestError::CaseIdentity(manifest.case_id));
    }
    if manifest.control_database != "app.db"
        || manifest.source_directory != "sources"
        || manifest.artifact_directory != "artifacts/objects"
    {
        return Err(StorageManifestError::Model(
            "storage layout mismatch".to_owned(),
        ));
    }
    Ok(manifest)
}
