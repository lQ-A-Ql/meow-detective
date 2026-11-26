use app_services::application_log_service::ApplicationLogService;
use tauri::State;
use transport::{dto::*, CommandError};

#[tauri::command]
pub async fn read_application_logs(
    logs: State<'_, ApplicationLogService>,
    request: ApplicationLogRequestDto,
) -> Result<ApplicationLogSnapshotDto, CommandError> {
    let logs = logs.inner().clone();
    tauri::async_runtime::spawn_blocking(move || logs.read(request))
        .await
        .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn get_application_log(
    logs: State<'_, ApplicationLogService>,
    session_id: String,
    sequence: u64,
) -> Result<ApplicationLogRecordDto, CommandError> {
    logs.record(&session_id, sequence)
}

#[tauri::command]
pub async fn reopen_application_log_writer(
    logs: State<'_, ApplicationLogService>,
) -> Result<ApplicationLogWriterDto, CommandError> {
    let logs = logs.inner().clone();
    tauri::async_runtime::spawn_blocking(move || logs.reopen_writer())
        .await
        .map_err(CommandError::from_join_error)?
}
