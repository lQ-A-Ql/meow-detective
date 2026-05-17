use transport::{commands::OpenFileHandleRequest, dto::{FileEntryRowDto, FileTreeNodeDto, ViewerHandleDto, ViewerRangeRequestDto, ViewerRangeResponseDto}};

#[tauri::command]
pub fn get_file_tree() -> Result<Vec<FileTreeNodeDto>, String> {
    Ok(app_services::file_service::get_file_tree())
}

#[tauri::command]
pub fn get_file_rows() -> Result<Vec<FileEntryRowDto>, String> {
    Ok(app_services::file_service::get_file_rows())
}

#[tauri::command]
pub fn open_file_handle(file_id: String) -> Result<ViewerHandleDto, String> {
    Ok(app_services::file_service::open_file_handle(file_id))
}

#[tauri::command]
pub fn open_file_handle_request(request: OpenFileHandleRequest) -> Result<ViewerHandleDto, String> {
    Ok(app_services::file_service::open_file_handle(request.file_id))
}

#[tauri::command]
pub fn read_file_range(request: ViewerRangeRequestDto) -> Result<ViewerRangeResponseDto, String> {
    Ok(app_services::file_service::read_file_range(request))
}
