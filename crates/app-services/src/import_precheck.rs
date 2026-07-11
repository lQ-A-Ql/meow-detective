//! Import pre-check and planning.
//!
//! Analyzes data sources before import to generate optimal import plans.

use crate::{
    datasource_service,
    import_state::{ImportPlan, ImportStrategy},
};
use domain::DataSourceKind;
use std::path::{Path, PathBuf};
use transport::commands::{ImportDataSourceRequest, ImportSourceKindDto, ImportTargetPlatformDto};

mod error;
pub use error::ImportSourceConfigError;

/// Bounded import configuration prepared before the Tauri job orchestration starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSourceConfig {
    pub source_path: PathBuf,
    pub source_path_display: String,
    pub source_name: String,
    pub kind: DataSourceKind,
    pub platform: Option<ImportTargetPlatformDto>,
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
    request: &ImportDataSourceRequest,
) -> Result<ImportSourceConfig, ImportSourceConfigError> {
    ensure_supported_import_platform(request.platform)?;
    request
        .validate()
        .map_err(ImportSourceConfigError::InvalidRequest)?;
    if request.source_kind != ImportSourceKindDto::Auto {
        return Err(ImportSourceConfigError::InvalidRequest(
            "non-auto sourceKind must be handled by the import scheduler".to_string(),
        ));
    }
    let mut config = prepare_import_source_config_from_path(&request.source_path)?;
    config.platform = request.platform;
    config.profile = request.profile.clone();
    Ok(config)
}

/// Rejects retired transport platform values before any source-path access.
pub fn ensure_supported_import_platform(
    platform: Option<ImportTargetPlatformDto>,
) -> Result<(), ImportSourceConfigError> {
    if platform == Some(ImportTargetPlatformDto::Unsupported) {
        return Err(ImportSourceConfigError::UnsupportedPlatform);
    }
    Ok(())
}

pub fn prepare_import_source_config_from_path(
    source_path: &str,
) -> Result<ImportSourceConfig, ImportSourceConfigError> {
    let path = PathBuf::from(source_path);
    validate_import_source_for_filesystem(&path)?;
    let kind = datasource_service::classify_data_source_path(&path)?;
    let source_name = derive_source_name(&path);
    let mode = import_source_mode(&kind);

    Ok(ImportSourceConfig {
        source_path: path,
        source_path_display: source_path.to_string(),
        source_name,
        kind,
        platform: None,
        profile: None,
        mode,
        cluster: None,
    })
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

fn import_source_mode(kind: &DataSourceKind) -> ImportSourceMode {
    match kind {
        DataSourceKind::LogicalDirectory => ImportSourceMode::LogicalDirectory,
        DataSourceKind::E01 => ImportSourceMode::Image {
            staging_kind: "E01",
        },
        DataSourceKind::Raw => ImportSourceMode::Image {
            staging_kind: "Raw",
        },
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
mod tests {
    use super::*;

    #[test]
    fn test_select_strategy_small() {
        assert_eq!(select_strategy(100, 1024), ImportStrategy::Sequential);
    }

    #[test]
    fn test_select_strategy_medium() {
        assert_eq!(
            select_strategy(5000, 1024),
            ImportStrategy::Parallel { workers: 4 }
        );
    }

    #[test]
    fn test_select_strategy_large_file() {
        assert_eq!(select_strategy(100, 500_000_000), ImportStrategy::Streaming);
    }

    #[test]
    fn test_estimate_files() {
        assert_eq!(estimate_files_from_size(100_000), 10);
        assert_eq!(estimate_files_from_size(1_000_000), 100);
    }

    #[test]
    fn test_pre_check_nonexistent() {
        let result = pre_import_check(Path::new("/nonexistent"), &DataSourceKind::LogicalDirectory);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_import_plan_time_estimate() {
        let plan = ImportPlan::new(ImportStrategy::Sequential, 1000, 1024 * 1024);
        assert!(plan.estimated_time_secs > 0);
    }

    #[test]
    fn test_import_plan_memory_estimate() {
        let plan = ImportPlan::new(
            ImportStrategy::Parallel { workers: 4 },
            10000,
            1024 * 1024 * 100,
        );
        assert!(plan.estimated_memory_bytes > 0);
    }

    #[test]
    fn import_source_config_classifies_logical_directory() {
        let tmp = tempfile::TempDir::new().unwrap();
        let evidence_dir = tmp.path().join("logical-evidence");
        std::fs::create_dir(&evidence_dir).unwrap();
        let request = ImportDataSourceRequest {
            source_path: evidence_dir.display().to_string(),
            source_kind: Default::default(),
            platform: None,
            profile: None,
        };

        let config = prepare_import_source_config(&request).unwrap();

        assert_eq!(config.source_path, evidence_dir);
        assert_eq!(config.source_name, "logical-evidence");
        assert_eq!(config.kind, DataSourceKind::LogicalDirectory);
        assert_eq!(config.mode, ImportSourceMode::LogicalDirectory);
        assert!(!config.is_image_backed());
        assert_eq!(config.staging_kind(), None);
    }

    #[test]
    fn import_source_config_preserves_optional_platform_profile_contract() {
        let tmp = tempfile::TempDir::new().unwrap();
        let evidence_dir = tmp.path().join("linux-logical");
        std::fs::create_dir(&evidence_dir).unwrap();
        let request = ImportDataSourceRequest {
            source_path: evidence_dir.display().to_string(),
            source_kind: Default::default(),
            platform: Some(ImportTargetPlatformDto::Linux),
            profile: Some("ubuntu-server".to_string()),
        };

        let config = prepare_import_source_config(&request).unwrap();

        assert_eq!(config.platform, Some(ImportTargetPlatformDto::Linux));
        assert_eq!(config.profile.as_deref(), Some("ubuntu-server"));
        assert_eq!(config.source_path, evidence_dir);
    }

    #[test]
    fn import_source_config_classifies_raw_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let source = tmp.path().join("disk.raw");
        std::fs::write(&source, b"not an e01 image").unwrap();
        let request = ImportDataSourceRequest {
            source_path: source.display().to_string(),
            source_kind: Default::default(),
            platform: None,
            profile: None,
        };

        let config = prepare_import_source_config(&request).unwrap();

        assert_eq!(config.source_path, source);
        assert_eq!(config.source_name, "disk.raw");
        assert_eq!(config.kind, DataSourceKind::Raw);
        assert_eq!(
            config.mode,
            ImportSourceMode::Image {
                staging_kind: "Raw"
            }
        );
        assert!(config.is_image_backed());
        assert_eq!(config.staging_kind(), Some("Raw"));
    }

    #[test]
    fn import_source_config_classifies_e01_by_name() {
        let tmp = tempfile::TempDir::new().unwrap();
        let source = tmp.path().join("capture.E01");
        std::fs::write(&source, b"short").unwrap();
        let request = ImportDataSourceRequest {
            source_path: source.display().to_string(),
            source_kind: Default::default(),
            platform: None,
            profile: None,
        };

        let config = prepare_import_source_config(&request).unwrap();

        assert_eq!(config.source_name, "capture.E01");
        assert_eq!(config.kind, DataSourceKind::E01);
        assert_eq!(
            config.mode,
            ImportSourceMode::Image {
                staging_kind: "E01"
            }
        );
        assert_eq!(config.staging_kind(), Some("E01"));
    }

    #[test]
    fn import_source_config_classifies_e01_by_magic() {
        let tmp = tempfile::TempDir::new().unwrap();
        let source = tmp.path().join("capture.bin");
        std::fs::write(&source, b"EVF\x09\x0d\x0a\xff\x00payload").unwrap();
        let request = ImportDataSourceRequest {
            source_path: source.display().to_string(),
            source_kind: Default::default(),
            platform: None,
            profile: None,
        };

        let config = prepare_import_source_config(&request).unwrap();

        assert_eq!(config.source_name, "capture.bin");
        assert_eq!(config.kind, DataSourceKind::E01);
        assert_eq!(config.staging_kind(), Some("E01"));
    }

    #[test]
    fn import_source_config_rejects_missing_source() {
        let tmp = tempfile::TempDir::new().unwrap();
        let request = ImportDataSourceRequest {
            source_path: tmp.path().join("missing.raw").display().to_string(),
            source_kind: Default::default(),
            platform: None,
            profile: None,
        };

        let error = prepare_import_source_config(&request).unwrap_err();

        assert!(matches!(
            error,
            ImportSourceConfigError::MissingOrInaccessibleSource
        ));
        assert!(error.is_invalid_input());
        assert_eq!(
            error.to_string(),
            "sourcePath must exist and be accessible before import"
        );
    }

    #[test]
    fn import_source_config_preserves_request_validation_semantics() {
        let request = ImportDataSourceRequest {
            source_path: "CON".to_string(),
            source_kind: Default::default(),
            platform: None,
            profile: None,
        };

        let error = prepare_import_source_config(&request).unwrap_err();

        assert!(matches!(error, ImportSourceConfigError::InvalidRequest(_)));
        assert!(error.is_invalid_input());
        assert_eq!(error.to_string(), "CON is a reserved Windows device name");
    }

    #[test]
    fn import_source_config_rejects_non_auto_source_kind() {
        let tmp = tempfile::TempDir::new().unwrap();
        let request = ImportDataSourceRequest {
            source_path: tmp.path().display().to_string(),
            source_kind: ImportSourceKindDto::LinuxCluster,
            platform: Some(ImportTargetPlatformDto::Linux),
            profile: None,
        };

        let error = prepare_import_source_config(&request).unwrap_err();

        assert!(matches!(error, ImportSourceConfigError::InvalidRequest(_)));
        assert!(error.to_string().contains("import scheduler"));
    }
}
