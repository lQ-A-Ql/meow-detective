use transport::{commands::SearchFilesRequest, dto::SearchResultPageDto};

#[tauri::command]
pub fn search_files(query: String) -> Result<SearchResultPageDto, String> {
    Ok(app_services::search_service::search_files(query))
}

#[tauri::command]
pub fn search_files_request(request: SearchFilesRequest) -> Result<SearchResultPageDto, String> {
    Ok(app_services::search_service::search_files(request.query))
}
