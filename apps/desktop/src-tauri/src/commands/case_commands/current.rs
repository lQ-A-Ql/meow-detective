use tauri::State;
use transport::{dto::CaseSummaryDto, CommandError};

use super::lifecycle_support::meta_to_dto;
use crate::state::AppState;

#[tauri::command]
pub fn get_current_case(state: State<AppState>) -> Result<Option<CaseSummaryDto>, CommandError> {
    let guard = state
        .active_case
        .lock()
        .map_err(|error| CommandError::from_lock_error("Case", error))?;
    Ok(guard.as_ref().map(|active| meta_to_dto(&active.meta)))
}
