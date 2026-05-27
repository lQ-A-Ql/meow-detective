use tauri::State;
use transport::dto::TimelineEventDto;

use crate::state::AppState;

#[tauri::command]
pub fn get_timeline_events(state: State<AppState>) -> Result<Vec<TimelineEventDto>, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    let Some(active) = guard.as_ref() else {
        return Ok(vec![]);
    };
    active
        .with_conn(|conn| {
            app_services::timeline_service::query_timeline(conn, 0, 100)
                .map_err(persistence_sqlite::DbError::System)
        })
        .map_err(|e| e.to_string())
}
