//! Rule pack command stubs.
//!
//! Provides placeholder Tauri commands for the rule-pack API surface so the
//! frontend receives well-shaped responses instead of runtime 404s.  No real
//! rule-pack registry is implemented here.

use serde::Deserialize;
use tauri::State;
use transport::dto::rule_pack::{
    RulePackCoverageDto, RulePackSummaryDto, RulePackValidationResultDto,
};
use transport::CommandError;

use super::command_support::{get_case_connection, snapshot_active_case};
use crate::state::AppState;

// ── Request DTOs ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadRulePackRequest {
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidateRulePackRequest {
    pub pack_id: String,
}

// ── Commands ────────────────────────────────────────────────────────────────

/// List all currently loaded rule packs.
#[tauri::command]
pub async fn list_loaded_rule_packs(
    state: State<'_, AppState>,
) -> Result<Vec<RulePackSummaryDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Keep the same active-case guard as other command files.
        if snapshot_active_case(&app_state)?.is_none() {
            return Ok(vec![]);
        }
        let _conn = get_case_connection(&app_state)?;
        Ok(vec![])
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Load a rule pack from the given path.
#[tauri::command]
pub async fn load_rule_pack(
    state: State<'_, AppState>,
    request: LoadRulePackRequest,
) -> Result<RulePackSummaryDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if snapshot_active_case(&app_state)?.is_none() {
            return Err(CommandError::no_active_case());
        }
        let _conn = get_case_connection(&app_state)?;

        let id = request
            .path
            .rsplit_once(['\\', '/'])
            .map(|(_, name)| name.to_string())
            .unwrap_or_else(|| request.path.clone());

        Ok(RulePackSummaryDto {
            id,
            name: request.path.clone(),
            version: "0.0.0".to_string(),
            author: None,
            description: None,
            status: "error".to_string(),
            rule_count: 0,
            loaded_at: "1970-01-01T00:00:00Z".to_string(),
            warnings: vec![],
            errors: vec!["Rule pack loading is not implemented".to_string()],
            covered_families: vec![],
        })
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Validate an already-loaded rule pack.
#[tauri::command]
pub async fn validate_rule_pack(
    state: State<'_, AppState>,
    request: ValidateRulePackRequest,
) -> Result<RulePackValidationResultDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if snapshot_active_case(&app_state)?.is_none() {
            return Err(CommandError::no_active_case());
        }
        let _conn = get_case_connection(&app_state)?;

        Ok(RulePackValidationResultDto {
            pack_id: request.pack_id,
            valid: false,
            errors: vec!["Validation is not implemented".to_string()],
            warnings: vec![],
            coverage: RulePackCoverageDto {
                covered_families: vec![],
                uncovered_families: vec![],
                coverage_percent: 0.0,
            },
        })
    })
    .await
    .map_err(CommandError::from_join_error)?
}
