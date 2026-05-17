use transport::dto::TimelineEventDto;

#[tauri::command]
pub fn get_timeline_events() -> Result<Vec<TimelineEventDto>, String> {
    Ok(app_services::timeline_service::get_timeline_events())
}
