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
pub fn validate_case_root_is_safe(case_root: &std::path::Path) -> Result<(), String> {
    let safe_root = safe_cases_root();

    // Ensure the safe root exists (create if needed)
    if !safe_root.exists() {
        std::fs::create_dir_all(&safe_root)
            .map_err(|e| format!("Failed to create cases root: {}", e))?;
    }

    let safe_canonical = safe_root
        .canonicalize()
        .map_err(|e| format!("Failed to resolve safe root: {}", e))?;

    let case_canonical = case_root
        .canonicalize()
        .map_err(|_| "Case path does not exist or cannot be resolved".to_string())?;

    if !case_canonical.starts_with(&safe_canonical) {
        return Err(format!(
            "Case path is outside the allowed cases directory ({}). \
             For security, cases can only be managed within this directory.",
            safe_canonical.display()
        ));
    }

    Ok(())
}
