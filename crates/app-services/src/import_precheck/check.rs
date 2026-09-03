use std::path::Path;

use domain::DataSourceKind;

use crate::import_state::{ImportPlan, ImportStrategy};

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
        DataSourceKind::E01 | DataSourceKind::Raw => {
            analyze_image(source_path, kind, &mut warnings)
        }
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
fn analyze_image(path: &Path, kind: &DataSourceKind, warnings: &mut Vec<String>) -> (u64, u64) {
    match image_logical_size(path, kind) {
        Ok(size) => {
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

fn image_logical_size(path: &Path, kind: &DataSourceKind) -> std::io::Result<u64> {
    match kind {
        DataSourceKind::Raw => evidence_core::RawImageReader::open(path).map(|reader| reader.len()),
        _ => std::fs::metadata(path).map(|metadata| metadata.len()),
    }
}

/// Estimate file count from image size
pub(crate) fn estimate_files_from_size(size: u64) -> u64 {
    // Rough heuristic: ~1 file per 10KB
    (size / 10_000).max(1)
}

/// Select optimal import strategy
pub(crate) fn select_strategy(total_files: u64, total_size: u64) -> ImportStrategy {
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
