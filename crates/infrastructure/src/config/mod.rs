//! Application configuration layer.

use std::path::PathBuf;

/// Returns the canonical root directory for storing forensic cases.
///
/// On Windows: `%APPDATA%/ForensicsWorkbench/cases/`
/// On other platforms: `$HOME/.forensics-workbench/cases/`
///
/// This is the only directory from which `delete_case` is permitted to
/// remove subdirectories, preventing arbitrary directory deletion.
pub fn safe_cases_root() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let appdata =
            std::env::var("APPDATA").unwrap_or_else(|_| r"C:\ForensicsWorkbench".to_string());
        PathBuf::from(appdata)
            .join("ForensicsWorkbench")
            .join("cases")
    }
    #[cfg(not(target_os = "windows"))]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home)
            .join(".forensics-workbench")
            .join("cases")
    }
}

/// Validate that a case root path is within the safe cases root directory.
///
/// Performs `canonicalize()` on both paths to resolve symlinks and `..` components,
/// then checks that `case_root` is a direct child of `safe_cases_root()`.
///
/// # Errors
/// Returns an error message if the path is outside the safe root or cannot be resolved.
/// Validate that a case root path is within a specific allowed root directory.
///
/// Performs `canonicalize()` on both paths to resolve symlinks and `..` components,
/// then checks that `case_root` starts with `allowed_root`.
///
/// # Errors
/// Returns an error message if the path is outside the allowed root or cannot be resolved.
pub fn validate_case_root_is_within(
    case_root: &std::path::Path,
    allowed_root: &std::path::Path,
) -> Result<(), String> {
    // Ensure the allowed root exists (create if needed)
    if !allowed_root.exists() {
        std::fs::create_dir_all(allowed_root)
            .map_err(|e| format!("Failed to create cases root: {}", e))?;
    }

    let allowed_canonical = allowed_root
        .canonicalize()
        .map_err(|e| format!("Failed to resolve allowed root: {}", e))?;

    let case_canonical = case_root
        .canonicalize()
        .map_err(|_| "Case path does not exist or cannot be resolved".to_string())?;

    if !case_canonical.starts_with(&allowed_canonical) {
        return Err(format!(
            "Case path is outside the allowed cases directory ({}). \
             For security, cases can only be managed within this directory.",
            allowed_canonical.display()
        ));
    }

    Ok(())
}

/// Validate that a case root path is within the default safe cases root directory.
///
/// Uses `safe_cases_root()` as the allowed root. See `validate_case_root_is_within`
/// for the parameterized version.
pub fn validate_case_root_is_safe(case_root: &std::path::Path) -> Result<(), String> {
    validate_case_root_is_within(case_root, &safe_cases_root())
}
