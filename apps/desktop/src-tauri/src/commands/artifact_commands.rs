use transport::{commands::GetArtifactRowsRequest, dto::ArtifactRowDto};

#[tauri::command]
pub fn get_artifact_families() -> Result<Vec<String>, String> {
    Ok(app_services::artifact_service::get_artifact_families())
}

#[tauri::command]
pub fn get_artifact_rows(family: Option<String>) -> Result<Vec<ArtifactRowDto>, String> {
    Ok(app_services::artifact_service::get_artifact_rows(family))
}

#[tauri::command]
pub fn get_artifact_rows_request(
    request: GetArtifactRowsRequest,
) -> Result<Vec<ArtifactRowDto>, String> {
    Ok(app_services::artifact_service::get_artifact_rows(
        request.family,
    ))
}
