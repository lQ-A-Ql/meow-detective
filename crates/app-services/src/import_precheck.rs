//! Import pre-check and planning.
//!
//! Analyzes data sources before import to generate optimal import plans.

use crate::{
    datasource_service,
    import_state::{ImportPlan, ImportStrategy},
};
use domain::{DataSourceKind, DataSourcePlatform};
use std::path::{Path, PathBuf};

mod error;
pub use error::ImportSourceConfigError;

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

/// Pre-check result
#[derive(Debug, Clone)]
pub struct PreCheckResult {
    /// Import plan
    pub plan: ImportPlan,
    /// Warnings (non-blocking)
    pub warnings: Vec<String>,
    /// Errors (blocking)
    pub errors: Vec<String>,
}

/// Perform pre-import analysis
pub fn pre_import_check(source_path: &Path, kind: &DataSourceKind) -> PreCheckResult {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    if matches!(kind, DataSourceKind::CephRbd) {
        errors.push("Ceph RBD derived data sources are not ordinary import sources".to_string());
        return PreCheckResult {
            plan: ImportPlan::new(ImportStrategy::Sequential, 0, 0),
            warnings,
            errors,
        };
    }

    // Check if path exists
    if !source_path.exists() {
        errors.push(format!("Path does not exist: {}", source_path.display()));
        return PreCheckResult {
            plan: ImportPlan::new(ImportStrategy::Sequential, 0, 0),
            warnings,
            errors,
        };
    }

    // Analyze based on type
    let (total_files, total_size) = match kind {
        DataSourceKind::LogicalDirectory => analyze_directory(source_path, &mut warnings),
        DataSourceKind::E01 | DataSourceKind::Raw => analyze_image(source_path, &mut warnings),
        DataSourceKind::CephRbd | DataSourceKind::CephFs => {
            unreachable!("derived Ceph sources are rejected before filesystem access")
        }
    };

    // Select strategy
    let strategy = select_strategy(total_files, total_size);

    // Add warnings for large imports
    if total_files > 100_000 {
        warnings.push(format!(
            "Large import: {} files. Consider using parallel mode.",
            total_files
        ));
    }

    if total_size > 10 * 1024 * 1024 * 1024 {
        warnings.push(format!(
            "Large data source: {:.1} GB. Streaming mode recommended.",
            total_size as f64 / (1024.0 * 1024.0 * 1024.0)
        ));
    }

    PreCheckResult {
        plan: ImportPlan::new(strategy, total_files, total_size),
        warnings,
        errors,
    }
}

/// Analyze a directory data source
fn analyze_directory(path: &Path, warnings: &mut Vec<String>) -> (u64, u64) {
    let mut total_files = 0u64;
    let mut total_size = 0u64;

    match std::fs::read_dir(path) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let metadata = entry.metadata();
                if let Ok(meta) = metadata {
                    if meta.is_dir() {
                        let (files, size) = count_directory_recursive(&entry.path());
                        total_files += files;
                        total_size += size;
                    } else {
                        total_files += 1;
                        total_size += meta.len();
                    }
                }
            }
        }
        Err(e) => {
            warnings.push(format!("Cannot read directory: {}", e));
        }
    }

    (total_files, total_size)
}

/// Recursively count files in directory
fn count_directory_recursive(path: &Path) -> (u64, u64) {
    let mut files = 0u64;
    let mut size = 0u64;

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_dir() {
                    let (f, s) = count_directory_recursive(&entry.path());
                    files += f;
                    size += s;
                } else {
                    files += 1;
                    size += meta.len();
                }
            }
        }
    }

    (files, size)
}

/// Analyze an image data source
fn analyze_image(path: &Path, warnings: &mut Vec<String>) -> (u64, u64) {
    match std::fs::metadata(path) {
        Ok(meta) => {
            let size = meta.len();
            // Estimate files based on size (rough heuristic)
            let estimated_files = estimate_files_from_size(size);
            (estimated_files, size)
        }
        Err(e) => {
            warnings.push(format!("Cannot read image file: {}", e));
            (0, 0)
        }
    }
}

/// Estimate file count from image size
fn estimate_files_from_size(size: u64) -> u64 {
    // Rough heuristic: ~1 file per 10KB
    (size / 10_000).max(1)
}

/// Select optimal import strategy
fn select_strategy(total_files: u64, total_size: u64) -> ImportStrategy {
    match (total_files, total_size) {
        // Small imports: sequential
        (0..=1000, 0..=100_000_000) => ImportStrategy::Sequential,
        // Medium imports: parallel
        (1001..=10000, _) => ImportStrategy::Parallel { workers: 4 },
        // Large files: streaming
        (_, 100_000_001..) => ImportStrategy::Streaming,
        // Default: adaptive
        _ => ImportStrategy::Adaptive,
    }
}

#[cfg(test)]
#[path = "../tests/unit/import_precheck.rs"]
mod tests;
