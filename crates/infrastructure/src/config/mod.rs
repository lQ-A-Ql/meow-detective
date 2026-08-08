//! Application configuration layer.

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
