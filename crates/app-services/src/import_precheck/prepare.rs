use std::path::{Path, PathBuf};

use domain::{DataSourceKind, DataSourcePlatform};

use super::ImportSourceConfigError;
use crate::datasource_service;

/// Bounded import configuration prepared before the Tauri job orchestration starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSourceConfig {
    pub source_path: PathBuf,
    pub source_path_display: String,
    pub source_name: String,
    pub kind: DataSourceKind,
    pub platform: DataSourcePlatform,
    pub profile: Option<String>,
    pub mode: ImportSourceMode,
    pub cluster: Option<ImportClusterMemberConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportClusterMemberConfig {
    pub cluster_id: String,
    pub member_index: u32,
    pub member_count: u32,
}

/// Import source mode derived from source classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportSourceMode {
    LogicalDirectory,
    Image { staging_kind: &'static str },
}

impl ImportSourceConfig {
    pub fn is_image_backed(&self) -> bool {
        matches!(self.mode, ImportSourceMode::Image { .. })
    }

    pub fn staging_kind(&self) -> Option<&'static str> {
        match self.mode {
            ImportSourceMode::LogicalDirectory => None,
            ImportSourceMode::Image { staging_kind } => Some(staging_kind),
        }
    }
}

pub fn prepare_import_source_config(
    source_path: &str,
    platform: DataSourcePlatform,
    profile: Option<String>,
) -> Result<ImportSourceConfig, ImportSourceConfigError> {
    let mut config = prepare_import_source_config_from_path(source_path, platform)?;
    config.profile = profile;
    Ok(config)
}

pub fn prepare_import_source_config_from_path(
    source_path: &str,
    platform: DataSourcePlatform,
) -> Result<ImportSourceConfig, ImportSourceConfigError> {
    ensure_supported_import_platform(platform)?;
    let path = PathBuf::from(source_path);
    validate_import_source_for_filesystem(&path)?;
    let kind = datasource_service::classify_data_source_path(&path)?;
    let source_name = derive_source_name(&path);
    let mode = import_source_mode(&kind).ok_or(ImportSourceConfigError::UnsupportedSourceType)?;

    Ok(ImportSourceConfig {
        source_path: path,
        source_path_display: source_path.to_string(),
        source_name,
        kind,
        platform,
        profile: None,
        mode,
        cluster: None,
    })
}

fn ensure_supported_import_platform(
    platform: DataSourcePlatform,
) -> Result<(), ImportSourceConfigError> {
    if platform == DataSourcePlatform::Unknown {
        return Err(ImportSourceConfigError::UnsupportedPlatform);
    }
    Ok(())
}

fn validate_import_source_for_filesystem(path: &Path) -> Result<(), ImportSourceConfigError> {
    let metadata = std::fs::metadata(path)
        .map_err(|_| ImportSourceConfigError::MissingOrInaccessibleSource)?;
    if metadata.is_dir() || metadata.is_file() {
        Ok(())
    } else {
        Err(ImportSourceConfigError::UnsupportedSourceType)
    }
}

fn derive_source_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "data_source".to_string())
}

fn import_source_mode(kind: &DataSourceKind) -> Option<ImportSourceMode> {
    match kind {
        DataSourceKind::LogicalDirectory => Some(ImportSourceMode::LogicalDirectory),
        DataSourceKind::E01 => Some(ImportSourceMode::Image {
            staging_kind: "E01",
        }),
        DataSourceKind::Raw => Some(ImportSourceMode::Image {
            staging_kind: "Raw",
        }),
        DataSourceKind::CephRbd | DataSourceKind::CephFs => None,
    }
}
