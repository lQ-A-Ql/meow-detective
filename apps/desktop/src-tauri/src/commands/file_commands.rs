//! File browsing and preview commands.
//!
//! Provides Tauri commands for:
//! - File tree navigation
//! - File listing and details
//! - File handle management for preview
//! - File range reading for hex/text viewer

use app_services::{file_service, text_service::TextService};
use base64::Engine;
use tauri::State;
use transport::{
    commands::{GetFileChildrenRequest, GetFileRowsRequest},
    dto::{FileEntryRowDto, FileTreeNodeDto, ImagePreviewDto, MediaUrlDto, TextPreviewDto, ViewerHandleDto, ViewerRangeResponseDto},
    CommandError,
};

use crate::state::AppState;

/// Get children of a file tree node (lazy loading).
#[tauri::command]
pub async fn get_file_children(
    state: State<'_, AppState>,
    parent_id: String,
) -> Result<Vec<FileTreeNodeDto>, CommandError> {
    get_file_children_request(state, GetFileChildrenRequest { parent_id }).await
}

/// Get children of a file tree node with explicit request.
#[tauri::command]
pub async fn get_file_children_request(
    state: State<'_, AppState>,
    request: GetFileChildrenRequest,
) -> Result<Vec<FileTreeNodeDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Short lock: extract db_path, then release
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => active.db_path(),
                None => return Ok(vec![]),
            }
        };
        // Guard is now dropped — query with released lock
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        file_service::get_file_children_lazy(&conn, &request.parent_id)
            .map(|result| result.children)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Get the complete file tree for the current case.
#[tauri::command]
pub async fn get_file_tree(
    state: State<'_, AppState>,
) -> Result<Vec<FileTreeNodeDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Short lock: extract db_path, then release
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => active.db_path(),
                None => return Ok(vec![]),
            }
        };
        // Guard is now dropped — query with released lock
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        file_service::get_file_tree_real(&conn).map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Get file rows for display in table view.
#[tauri::command]
pub async fn get_file_rows(
    state: State<'_, AppState>,
) -> Result<Vec<FileEntryRowDto>, CommandError> {
    get_file_rows_request(state, GetFileRowsRequest::default()).await
}

/// Get file rows with explicit request parameters.
#[tauri::command]
pub async fn get_file_rows_request(
    state: State<'_, AppState>,
    request: GetFileRowsRequest,
) -> Result<Vec<FileEntryRowDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Short lock: extract db_path, then release
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => active.db_path(),
                None => return Ok(vec![]),
            }
        };
        // Guard is now dropped — query with released lock
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        file_service::get_file_rows_for_request(&conn, &request)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Open a file handle for preview (returns handle ID and metadata).
#[tauri::command]
pub async fn open_file_handle(
    state: State<'_, AppState>,
    file_id: String,
) -> Result<ViewerHandleDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Short lock: extract db_path, then release
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            let active = guard.as_ref().ok_or_else(CommandError::no_active_case)?;
            active.db_path()
        };
        // Guard is now dropped — query with released lock
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        file_service::open_file_handle_real(&conn, &file_id)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Open a file handle with explicit request.
#[tauri::command]
pub async fn open_file_handle_request(
    state: State<'_, AppState>,
    request: transport::commands::OpenFileHandleRequest,
) -> Result<ViewerHandleDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Short lock: extract db_path, then release
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            let active = guard.as_ref().ok_or_else(CommandError::no_active_case)?;
            active.db_path()
        };
        // Guard is now dropped — query with released lock
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        file_service::open_file_handle_real(&conn, &request.file_id)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Read a range of bytes from a file (for hex/text viewer).
#[tauri::command]
pub async fn read_file_range(
    state: State<'_, AppState>,
    request: transport::dto::ViewerRangeRequestDto,
) -> Result<ViewerRangeResponseDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Short lock: extract db_path, then release
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => active.db_path(),
                None => return Ok(file_service::read_file_range_real(&request)),
            }
        };
        // Guard is now dropped — query with released lock
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        file_service::read_file_range_for_case(&conn, &request)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Get text preview for a file.
///
/// Returns text content with encoding detection.
/// Returns binary indicator for non-text files.
#[tauri::command]
pub async fn get_text_preview(
    state: State<'_, AppState>,
    file_id: String,
    max_bytes: Option<usize>,
) -> Result<TextPreviewDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let max = max_bytes.unwrap_or(1024 * 1024) as u32; // 默认 1MB

        // Short lock: extract db_path, then release
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => active.db_path(),
                None => return Err(CommandError::no_active_case()),
            }
        };

        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;

        // Get file handle
        let handle = file_service::open_file_handle_real(&conn, &file_id)
            .map_err(CommandError::from_service_error)?;

        // Read file content
        let range = file_service::read_file_range_for_case(
            &conn,
            &transport::dto::ViewerRangeRequestDto {
                handle_id: handle.handle_id.clone(),
                offset: 0,
                length: max,
            },
        )
        .map_err(CommandError::from_service_error)?;

        // Decode hex lines to bytes
        // Format: "00000000  48 65 6C 6C 6F  ..."
        let content_bytes: Vec<u8> = range
            .lines
            .iter()
            .filter(|line| !line.trim().is_empty()) // Skip empty lines
            .flat_map(|line| {
                line.split_whitespace()
                    .skip(1) // Skip offset
                    .take_while(|hex| hex.len() <= 2) // Stop at non-hex tokens
                    .filter_map(|hex| u8::from_str_radix(hex, 16).ok())
                    .collect::<Vec<u8>>()
            })
            .collect();

        // Detect encoding and extract text
        let preview = TextService::extract_text_preview(
            &mut std::io::Cursor::new(&content_bytes),
            max as usize,
        )
        .map_err(|e| CommandError::from_service_error(e.to_string()))?;

        Ok(TextPreviewDto {
            content: preview.content,
            encoding: preview.encoding,
            is_truncated: preview.is_truncated,
            line_count: preview.line_count,
            is_binary: preview.is_binary,
            language: preview.language,
        })
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Get image preview for a file.
///
/// Returns base64-encoded image data for display.
#[tauri::command]
pub async fn get_image_preview(
    state: State<'_, AppState>,
    file_id: String,
) -> Result<ImagePreviewDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Short lock: extract db_path, then release
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => active.db_path(),
                None => return Err(CommandError::no_active_case()),
            }
        };

        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;

        // Get file handle
        let handle = file_service::open_file_handle_real(&conn, &file_id)
            .map_err(CommandError::from_service_error)?;

        // Check if it's an image
        let mime = handle.mime.as_deref().unwrap_or("");
        if !mime.starts_with("image/") {
            return Err(CommandError::from_service_error("Not an image file"));
        }

        // Read file content
        let range = file_service::read_file_range_for_case(
            &conn,
            &transport::dto::ViewerRangeRequestDto {
                handle_id: handle.handle_id.clone(),
                offset: 0,
                length: handle.size as u32,
            },
        )
        .map_err(CommandError::from_service_error)?;

        // Decode hex lines to bytes
        // Format: "00000000  48 65 6C 6C 6F  ..."
        let content_bytes: Vec<u8> = range
            .lines
            .iter()
            .filter(|line| !line.trim().is_empty()) // Skip empty lines
            .flat_map(|line| {
                line.split_whitespace()
                    .skip(1) // Skip offset
                    .take_while(|hex| hex.len() <= 2) // Stop at non-hex tokens
                    .filter_map(|hex| u8::from_str_radix(hex, 16).ok())
                    .collect::<Vec<u8>>()
            })
            .collect();

        // Base64 encode
        let base64 = base64::engine::general_purpose::STANDARD.encode(&content_bytes);

        Ok(ImagePreviewDto {
            data_url: format!("data:{};base64,{}", mime, base64),
            mime_type: mime.to_string(),
            width: 0,  // Frontend will detect
            height: 0, // Frontend will detect
            size: handle.size,
        })
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Get media URL for video/audio playback.
///
/// Returns a local file URL that can be used by the browser's media player.
#[tauri::command]
pub async fn get_media_url(
    state: State<'_, AppState>,
    file_id: String,
) -> Result<MediaUrlDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Short lock: extract db_path, then release
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => active.db_path(),
                None => return Err(CommandError::no_active_case()),
            }
        };

        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;

        // Get file handle
        let handle = file_service::open_file_handle_real(&conn, &file_id)
            .map_err(CommandError::from_service_error)?;

        // Get file path
        let file_path = file_service::get_file_path_for_entry(&conn, &file_id)
            .map_err(CommandError::from_service_error)?;

        // Create asset URL for Tauri with proper encoding
        let path_str = file_path.to_string_lossy();
        let url = format!("asset://localhost/{}", path_str.replace(' ', "%20"));

        Ok(MediaUrlDto {
            url,
            mime_type: handle.mime.unwrap_or_else(|| "application/octet-stream".to_string()),
            size: handle.size,
        })
    })
    .await
    .map_err(CommandError::from_join_error)?
}
