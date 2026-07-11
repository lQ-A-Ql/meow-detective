use std::path::{Path, PathBuf};
use transport::{
    commands::DeleteCaseRequest,
    dto::{CaseSummaryDto, RecentCaseDto},
    CommandError,
};

const APP_CODE_NAME: &str = "Meow_Detective";
const RECENT_CASES_FILE: &str = "Meow_Detective-recent-cases.json";
const MAX_RECENT_CASES: usize = 8;

#[tauri::command]
pub fn get_recent_cases() -> Result<Vec<RecentCaseDto>, CommandError> {
    read_recent_cases()
}

#[tauri::command]
pub fn remove_case_from_list(request: DeleteCaseRequest) -> Result<String, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let mut recent = read_recent_cases().unwrap_or_else(|error| {
        tracing::warn!("Failed to read recent cases, starting fresh: {}", error);
        Vec::new()
    });
    recent.retain(|item| item.case_root != request.case_root);
    save_recent_cases(&recent)?;
    Ok(format!("Removed from list: {}", request.case_root))
}

pub(super) fn remember_recent_case(
    case_root: &Path,
    summary: &CaseSummaryDto,
) -> Result<(), CommandError> {
    let mut recent = read_recent_cases().unwrap_or_else(|error| {
        tracing::warn!("Failed to read recent cases, starting fresh: {}", error);
        Vec::new()
    });
    recent.retain(|item| item.case_root != case_root.display().to_string());
    recent.insert(
        0,
        RecentCaseDto {
            case_root: case_root.display().to_string(),
            name: summary.name.clone(),
            opened_at: chrono::Utc::now().to_rfc3339(),
        },
    );
    recent.truncate(MAX_RECENT_CASES);
    save_recent_cases(&recent)
}

pub(super) fn read_recent_cases() -> Result<Vec<RecentCaseDto>, CommandError> {
    let path = recent_cases_path()?;
    if !path.exists() {
        return Ok(vec![]);
    }

    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) => {
            tracing::warn!("Failed to read recent cases file: {}", error);
            return Ok(vec![]);
        }
    };
    let parsed: Vec<RecentCaseDto> = match serde_json::from_str(&content) {
        Ok(parsed) => parsed,
        Err(error) => {
            tracing::warn!("Failed to parse recent cases JSON: {}", error);
            return Ok(vec![]);
        }
    };
    Ok(parsed
        .into_iter()
        .filter(|item| valid_recent_case_root(&item.case_root))
        .take(MAX_RECENT_CASES)
        .collect())
}

pub(super) fn save_recent_cases(recent: &[RecentCaseDto]) -> Result<(), CommandError> {
    let path = recent_cases_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            tracing::error!("Failed to create recent cases directory: {}", error);
            CommandError::internal("Failed to save recent cases")
        })?;
    }
    let json = serde_json::to_string_pretty(recent).map_err(|error| {
        tracing::error!("Failed to serialize recent cases: {}", error);
        CommandError::internal("Failed to save recent cases")
    })?;
    std::fs::write(&path, json).map_err(|error| {
        tracing::error!("Failed to write recent cases file: {}", error);
        CommandError::internal("Failed to save recent cases")
    })?;

    if let Err(error) = crate::platform_security::restrict_file_to_current_user(&path) {
        tracing::error!("Failed to restrict recent cases file ACL: {}", error);
        return Err(CommandError::security("Failed to secure recent cases file"));
    }

    Ok(())
}

pub(super) fn recent_cases_path() -> Result<PathBuf, CommandError> {
    // FORENSICS_RECENT_CASES_DIR is intended for tests; in production it is not set.
    let base = std::env::var_os("FORENSICS_RECENT_CASES_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .ok_or_else(|| CommandError::internal("Cannot resolve APPDATA for recent cases"))?;
    Ok(base.join(APP_CODE_NAME).join(RECENT_CASES_FILE))
}

fn valid_recent_case_root(case_root: &str) -> bool {
    if case_root.trim().is_empty() || case_root.contains('\0') {
        return false;
    }
    let root = PathBuf::from(case_root);
    root.is_dir() && root.join("case.json").is_file() && root.join("app.db").is_file()
}
