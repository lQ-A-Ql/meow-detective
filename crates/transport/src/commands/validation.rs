pub(super) const MAX_PAGE_LIMIT: u32 = 500;
pub(super) const DEFAULT_PAGE_LIMIT: u32 = 100;

const APP_CODE_NAME: &str = "Meow_Detective";

pub(super) fn validate_required_data_source_id(data_source_id: &str) -> Result<(), String> {
    if data_source_id.trim().is_empty() {
        return Err("dataSourceId is required".to_string());
    }
    Ok(())
}

pub(super) fn validate_import_source_path(path: &str) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("sourcePath is required".to_string());
    }
    if trimmed.contains('\0') {
        return Err("sourcePath contains a null byte".to_string());
    }

    let normalized = trimmed.replace('/', "\\");
    let upper = normalized.to_ascii_uppercase();
    if upper.starts_with("\\\\.\\") {
        return Err("Windows device paths are not supported".to_string());
    }
    if upper.starts_with("\\\\?\\") {
        return Err("Extended-length Windows paths are not supported".to_string());
    }

    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    for component in normalized
        .split('\\')
        .filter(|component| !component.is_empty())
    {
        let stem = component
            .split('.')
            .next()
            .unwrap_or(component)
            .trim_end_matches(' ')
            .to_ascii_uppercase();
        if reserved.contains(&stem.as_str()) {
            return Err(format!("{stem} is a reserved Windows device name"));
        }
    }

    Ok(())
}

pub(super) fn validate_export_destination_path(path: &str) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("destinationPath is required".to_string());
    }
    if trimmed.contains('\0') {
        return Err("destinationPath contains a null byte".to_string());
    }
    let normalized = trimmed.replace('/', "\\");
    let upper = normalized.to_ascii_uppercase();
    if upper.starts_with("\\\\.\\") || upper.starts_with("\\\\?\\") {
        return Err("device destination paths are not supported".to_string());
    }
    Ok(())
}

pub(super) fn validate_config_directory_path(
    field: &str,
    path: &str,
    must_exist: bool,
) -> Result<(), String> {
    validate_import_source_path(path)?;
    let metadata =
        std::fs::metadata(path).map_err(|_| format!("{field} must exist and be accessible"))?;
    if !metadata.is_dir() {
        return Err(format!("{field} must point to a directory"));
    }
    if must_exist {
        std::fs::read_dir(path).map_err(|_| format!("{field} must be a readable directory"))?;
    }
    Ok(())
}

pub(super) fn default_case_root() -> String {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .map(|root| format!("{root}\\{APP_CODE_NAME}\\cases"))
            .unwrap_or_else(|_| format!("C:\\{APP_CODE_NAME}\\cases"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .map(|root| format!("{root}/.{APP_CODE_NAME}/cases"))
            .unwrap_or_else(|_| format!("/tmp/.{APP_CODE_NAME}/cases"))
    }
}
