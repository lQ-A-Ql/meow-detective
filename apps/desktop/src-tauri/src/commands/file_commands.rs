//! File browsing and preview commands.
//!
//! Provides Tauri commands for:
//! - File tree navigation
//! - File listing and details
//! - File handle management for preview
//! - File range reading for hex/text viewer

use app_services::{file_service, text_service::TextService};
use base64::Engine;
use persistence_sqlite::repositories::audit_repo::{AuditAction, AuditRepo};
use std::io::Read;
use std::io::Write;
use tauri::State;
use transport::{
    commands::{
        ExtractFileRequest, GetFileChildrenRequest, GetFileJumpContextRequest, GetFileRowsRequest,
        GetFileTreeRequest,
    },
    dto::{
        FileChildrenDto, FileEntryRowDto, FileJumpContextDto, FileRowsPageDto, FileTreeNodeDto,
        ImagePreviewDto, MediaPreviewModeDto, MediaRangeRequestDto, MediaRangeResponseDto,
        MediaUrlDto, TextPreviewDto, ViewerHandleDto, ViewerRangeResponseDto,
    },
    CommandError,
};

#[cfg(test)]
use transport::dto::MAX_VIEWER_RANGE_LENGTH;

use crate::state::AppState;

fn current_case_id(state: &AppState) -> Option<String> {
    state
        .active_case
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|active| active.meta.id.0.clone()))
}

fn write_file_audit(
    state: &AppState,
    action: AuditAction,
    resource_id: Option<&str>,
    details: serde_json::Value,
) {
    let Ok(conn) = state.get_connection() else {
        return;
    };
    let case_id = current_case_id(state);
    let details_str = serde_json::to_string(&details).unwrap_or_else(|_| "{}".to_string());
    let _ = AuditRepo::new(&conn).log(
        case_id.as_deref(),
        "system",
        &action,
        resource_id,
        &details_str,
    );
}

/// Get children of a file tree node (lazy loading).
#[tauri::command]
pub async fn get_file_children(
    state: State<'_, AppState>,
    parent_id: String,
) -> Result<Vec<FileTreeNodeDto>, CommandError> {
    let page = get_file_children_request(
        state,
        GetFileChildrenRequest {
            parent_id,
            offset: 0,
            limit: infrastructure::constants::MAX_PAGE_LIMIT,
            show_hidden: false,
        },
    )
    .await?;
    Ok(page.children)
}

/// Get children of a file tree node with explicit request.
#[tauri::command]
pub async fn get_file_children_request(
    state: State<'_, AppState>,
    mut request: GetFileChildrenRequest,
) -> Result<FileChildrenDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if crate::commands::command_support::snapshot_active_case(&app_state)?.is_none() {
            return Ok(FileChildrenDto {
                children: vec![],
                total_count: 0,
                offset: Some(request.offset),
                limit: Some(request.limit),
                truncated: Some(false),
            });
        }
        let conn = crate::commands::command_support::get_case_connection(&app_state)?;
        file_service::get_file_children_lazy_with_visibility(
            &conn,
            &request.parent_id,
            request.offset,
            request.limit,
            request.show_hidden,
        )
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
    let page = get_file_tree_request(state, GetFileTreeRequest::default()).await?;
    Ok(page)
}

/// Get the complete file tree for the current case with explicit visibility.
#[tauri::command]
pub async fn get_file_tree_request(
    state: State<'_, AppState>,
    request: GetFileTreeRequest,
) -> Result<Vec<FileTreeNodeDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if crate::commands::command_support::snapshot_active_case(&app_state)?.is_none() {
            return Ok(vec![]);
        }
        let conn = crate::commands::command_support::get_case_connection(&app_state)?;
        file_service::get_file_tree_real_with_visibility(&conn, request.show_hidden)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Get file rows for display in table view.
#[tauri::command]
pub async fn get_file_rows(
    state: State<'_, AppState>,
) -> Result<Vec<FileEntryRowDto>, CommandError> {
    let page = get_file_rows_request(state, GetFileRowsRequest::default()).await?;
    Ok(page.rows)
}

/// Get file rows with explicit request parameters.
#[tauri::command]
pub async fn get_file_rows_request(
    state: State<'_, AppState>,
    mut request: GetFileRowsRequest,
) -> Result<FileRowsPageDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if crate::commands::command_support::snapshot_active_case(&app_state)?.is_none() {
            return Ok(FileRowsPageDto {
                rows: vec![],
                total_count: 0,
                offset: request.offset,
                limit: request.limit,
                truncated: false,
            });
        }
        let conn = crate::commands::command_support::get_case_connection(&app_state)?;
        file_service::get_file_rows_for_request(&conn, &request)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Resolve a file jump target into directory context and row page offset.
#[tauri::command]
pub async fn get_file_jump_context(
    state: State<'_, AppState>,
    mut request: GetFileJumpContextRequest,
) -> Result<FileJumpContextDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = crate::commands::command_support::get_case_connection(&app_state)?;
        file_service::get_file_jump_context(&conn, &request).map_err(|err| {
            if err.to_string().contains("not found") {
                CommandError::not_found("File")
            } else {
                CommandError::from_service_error(err)
            }
        })
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
        let conn = crate::commands::command_support::get_case_connection(&app_state)?;
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
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = crate::commands::command_support::get_case_connection(&app_state)?;
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
    mut request: transport::dto::ViewerRangeRequestDto,
) -> Result<ViewerRangeResponseDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if crate::commands::command_support::snapshot_active_case(&app_state)?.is_none() {
            return Ok(file_service::read_file_range_real(&request));
        }
        let conn = crate::commands::command_support::get_case_connection(&app_state)?;
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
        let max =
            max_bytes.unwrap_or(infrastructure::constants::DEFAULT_TEXT_PREVIEW_MAX_BYTES) as u32;

        let conn = crate::commands::command_support::get_case_connection(&app_state)?;

        let content_bytes = file_service::read_file_header_by_id(
            &conn,
            &domain::FileEntryId(file_id.clone()),
            max as usize,
        )
        .map_err(CommandError::from_service_error)?;

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
        let conn = crate::commands::command_support::get_case_connection(&app_state)?;

        // Get file handle
        let handle = file_service::open_file_handle_real(&conn, &file_id)
            .map_err(CommandError::from_service_error)?;

        // Check if it's an image
        let mime = handle.mime.as_deref().unwrap_or("");
        if !mime.starts_with("image/") {
            return Err(CommandError::from_service_error("Not an image file"));
        }

        if handle.size > infrastructure::constants::MAX_INLINE_IMAGE_PREVIEW_BYTES {
            return Err(CommandError::invalid_input(format!(
                "Image preview is limited to {} MB",
                infrastructure::constants::MAX_INLINE_IMAGE_PREVIEW_BYTES
                    / infrastructure::constants::BYTES_PER_MB
            )));
        }

        let mut reader =
            file_service::open_file_content_by_id(&conn, &domain::FileEntryId(file_id.clone()))
                .map_err(CommandError::from_service_error)?;
        let mut content_bytes = Vec::with_capacity(handle.size as usize);
        reader
            .read_to_end(&mut content_bytes)
            .map_err(CommandError::from_service_error)?;

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
/// Returns a bounded inline data URL instead of exposing host filesystem paths.
#[tauri::command]
pub async fn get_media_url(
    state: State<'_, AppState>,
    file_id: String,
) -> Result<MediaUrlDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = crate::commands::command_support::get_case_connection(&app_state)?;

        media_data_url_for_file(&app_state, &conn, &file_id)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Read a bounded raw byte range for media preview.
#[tauri::command]
pub async fn read_media_range(
    state: State<'_, AppState>,
    mut request: MediaRangeRequestDto,
) -> Result<MediaRangeResponseDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = crate::commands::command_support::get_case_connection(&app_state)?;

        media_range_for_file(&app_state, &conn, &request)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Extract a file from evidence to a user-selected destination path.
#[tauri::command]
pub async fn extract_file(
    state: State<'_, AppState>,
    request: ExtractFileRequest,
) -> Result<String, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let audit_file_id = request.file_id.clone();
    let audit_destination = request.destination_path.clone();
    let overwrite = request.overwrite;
    tauri::async_runtime::spawn_blocking(move || {
        let conn = crate::commands::command_support::get_case_connection(&app_state)?;
        let result = extract_file_for_case(&conn, &request);
        match &result {
            Ok(message) => write_file_audit(
                &app_state,
                AuditAction::FileExtract,
                Some(&audit_file_id),
                serde_json::json!({
                    "status": "ok",
                    "overwrite": overwrite,
                    "destinationFileName": std::path::Path::new(&audit_destination)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("unknown"),
                    "message": message,
                }),
            ),
            Err(err) => write_file_audit(
                &app_state,
                AuditAction::FileExtract,
                Some(&audit_file_id),
                serde_json::json!({
                    "status": "failed",
                    "overwrite": overwrite,
                    "destinationFileName": std::path::Path::new(&audit_destination)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("unknown"),
                    "errorCode": err.code,
                    "errorCategory": err.category,
                }),
            ),
        }
        result
    })
    .await
    .map_err(CommandError::from_join_error)?
}

fn extract_file_for_case(
    conn: &rusqlite::Connection,
    request: &ExtractFileRequest,
) -> Result<String, CommandError> {
    let mut reader =
        file_service::open_file_content_by_id(conn, &domain::FileEntryId(request.file_id.clone()))
            .map_err(CommandError::from_service_error)?;
    let destination = std::path::PathBuf::from(&request.destination_path);
    if destination.exists() && destination.is_dir() {
        return Err(CommandError::invalid_input(
            "destinationPath must point to a file, not a directory",
        ));
    }
    if destination.exists() && !request.overwrite {
        return Err(CommandError::conflict(
            "destinationPath already exists; set overwrite=true to replace it",
        ));
    }
    if let Some(parent) = destination.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(CommandError::from_service_error)?;
        }
    }
    let temp_path = destination.with_extension(format!(
        "{}{}.tmp",
        destination
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| format!("{ext}."))
            .unwrap_or_default(),
        uuid::Uuid::new_v4()
    ));
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(CommandError::from_service_error)?;
    let bytes = std::io::copy(&mut reader, &mut output).map_err(|err| {
        let _ = std::fs::remove_file(&temp_path);
        CommandError::from_service_error(err)
    })?;
    output.flush().map_err(|err| {
        let _ = std::fs::remove_file(&temp_path);
        CommandError::from_service_error(err)
    })?;
    output.sync_all().map_err(|err| {
        let _ = std::fs::remove_file(&temp_path);
        CommandError::from_service_error(err)
    })?;
    drop(output);

    if request.overwrite && destination.exists() {
        std::fs::remove_file(&destination).map_err(|err| {
            let _ = std::fs::remove_file(&temp_path);
            CommandError::from_service_error(err)
        })?;
    }
    std::fs::rename(&temp_path, &destination).map_err(|err| {
        let _ = std::fs::remove_file(&temp_path);
        CommandError::from_service_error(err)
    })?;
    Ok(format!("Extracted {} bytes", bytes))
}

fn media_data_url_for_file(
    state: &AppState,
    conn: &rusqlite::Connection,
    file_id: &str,
) -> Result<MediaUrlDto, CommandError> {
    let handle = file_service::open_file_handle_real(conn, file_id)
        .map_err(CommandError::from_service_error)?;
    let mime = handle
        .mime
        .clone()
        .unwrap_or_else(|| "application/octet-stream".to_string());
    if handle.size > infrastructure::constants::MAX_INLINE_MEDIA_PREVIEW_BYTES {
        let scoped_handle = crate::media_protocol::create_scoped_media_handle(state, file_id)
            .map_err(CommandError::security)?;
        return Ok(MediaUrlDto {
            mode: MediaPreviewModeDto::Protocol,
            url: Some(crate::media_protocol::media_protocol_url(&scoped_handle)),
            handle_id: Some(scoped_handle),
            mime_type: mime,
            size: handle.size,
            can_read_ranges: true,
        });
    }

    let mut reader =
        file_service::open_file_content_by_id(conn, &domain::FileEntryId(file_id.to_string()))
            .map_err(CommandError::from_service_error)?;
    let mut content_bytes = Vec::with_capacity(handle.size as usize);
    reader
        .read_to_end(&mut content_bytes)
        .map_err(CommandError::from_service_error)?;
    let base64 = base64::engine::general_purpose::STANDARD.encode(&content_bytes);

    Ok(MediaUrlDto {
        mode: MediaPreviewModeDto::Inline,
        url: Some(format!("data:{};base64,{}", mime, base64)),
        handle_id: Some(handle.handle_id),
        mime_type: mime,
        size: handle.size,
        can_read_ranges: true,
    })
}

fn media_range_for_file(
    state: &AppState,
    conn: &rusqlite::Connection,
    request: &MediaRangeRequestDto,
) -> Result<MediaRangeResponseDto, CommandError> {
    let file_id = crate::media_protocol::resolve_scoped_media_handle(state, &request.handle_id)
        .map_err(CommandError::security)?;
    let handle = file_service::open_file_handle_real(conn, &file_id)
        .map_err(CommandError::from_service_error)?;
    if request.offset >= handle.size {
        return Ok(MediaRangeResponseDto {
            offset: request.offset,
            bytes_base64: String::new(),
            bytes_read: 0,
            eof: true,
        });
    }
    let mut reader =
        file_service::open_file_content_by_id(conn, &domain::FileEntryId(file_id.clone()))
            .map_err(CommandError::from_service_error)?;

    file_service::skip_reader_bytes(reader.as_mut(), request.offset)
        .map_err(CommandError::from_service_error)?;
    let readable_len = request.length.min((handle.size - request.offset) as u32);
    let mut bytes = vec![0u8; readable_len as usize];
    let bytes_read = reader
        .read(&mut bytes)
        .map_err(CommandError::from_service_error)?;
    bytes.truncate(bytes_read);
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let end_offset = request.offset.saturating_add(bytes_read as u64);

    Ok(MediaRangeResponseDto {
        offset: request.offset,
        bytes_base64: encoded,
        bytes_read: bytes_read as u32,
        eof: end_offset >= handle.size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_services::{case_service, file_service};
    use evidence_core::LogicalFsReader;
    use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
    use tempfile::TempDir;

    fn test_state_with_case(case_id: &str) -> AppState {
        let state = AppState::default();
        let conn = persistence_sqlite::open_in_memory().expect("runtime cache test db");
        let active = app_services::active_case::ActiveCase::new(
            domain::CaseMeta {
                id: domain::CaseId(case_id.to_string()),
                name: "Test Case".to_string(),
                number: None,
                examiner: None,
                notes: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
            std::env::temp_dir().join(format!("forensics-media-test-{case_id}")),
            conn,
        );
        *state.active_case.lock().expect("active case lock") = Some(active);
        state
    }

    fn with_logical_case_file(
        case_name: &str,
        file_name: &str,
        content: &[u8],
        test: impl FnOnce(
            &rusqlite::Connection,
            String,
            std::path::PathBuf,
        ) -> Result<(), persistence_sqlite::DbError>,
    ) {
        let tmp = TempDir::new().unwrap();
        let evidence_dir = tmp.path().join("evidence");
        std::fs::create_dir_all(&evidence_dir).unwrap();
        std::fs::write(evidence_dir.join(file_name), content).unwrap();

        let active =
            case_service::create_case(&tmp.path().join("cases"), case_name, Some("tester"))
                .unwrap();
        let case_id = active.meta.id.clone();

        active
            .with_conn(|conn| {
                let ds_id = domain::DataSourceId("ds-media".to_string());
                DataSourceRepo::new(conn).insert(
                    &case_id,
                    &domain::DataSource {
                        id: ds_id.clone(),
                        name: "evidence".to_string(),
                        kind: domain::DataSourceKind::LogicalDirectory,
                        source_path: evidence_dir.clone(),
                        imported_at: chrono::Utc::now(),
                        provenance: domain::DataSourceProvenance::unknown(),
                    },
                )?;

                let fs = LogicalFsReader::open(&evidence_dir, "evidence")
                    .map_err(|err| persistence_sqlite::DbError::System(err.to_string()))?;
                file_service::enumerate_filesystem(conn, &ds_id, &fs)?;

                let file_id = persistence_sqlite::repositories::file_repo::FileRepo::new(conn)
                    .find_by_data_source(&ds_id)?
                    .into_iter()
                    .find(|entry| entry.name == file_name)
                    .map(|entry| entry.id.0)
                    .expect("file should be enumerated");

                test(conn, file_id, evidence_dir.clone())?;
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn media_preview_returns_data_url_without_host_path() {
        with_logical_case_file(
            "media",
            "clip.mp4",
            b"tiny media bytes",
            |conn, file_id, evidence_dir| {
                let state = test_state_with_case("case-media-inline");
                let media = media_data_url_for_file(&state, conn, &file_id)
                    .map_err(|err| persistence_sqlite::DbError::System(err.message))?;
                let url = media.url.expect("small media should return inline URL");
                assert!(url.starts_with("data:"));
                assert!(!url.starts_with("file:"));
                assert!(!url.starts_with("asset://"));
                assert!(!url.contains(&evidence_dir.to_string_lossy().to_string()));
                assert!(media.can_read_ranges);

                Ok(())
            },
        );
    }

    #[test]
    fn extract_file_uses_entry_reader_and_writes_destination() {
        let tmp = TempDir::new().unwrap();
        let evidence_dir = tmp.path().join("evidence");
        std::fs::create_dir_all(&evidence_dir).unwrap();
        std::fs::write(evidence_dir.join("note.txt"), b"extract me").unwrap();

        let active =
            case_service::create_case(&tmp.path().join("cases"), "extract", Some("tester"))
                .unwrap();
        let case_id = active.meta.id.clone();

        active
            .with_conn(|conn| {
                let ds_id = domain::DataSourceId("ds-extract".to_string());
                DataSourceRepo::new(conn).insert(
                    &case_id,
                    &domain::DataSource {
                        id: ds_id.clone(),
                        name: "evidence".to_string(),
                        kind: domain::DataSourceKind::LogicalDirectory,
                        source_path: evidence_dir.clone(),
                        imported_at: chrono::Utc::now(),
                        provenance: domain::DataSourceProvenance::unknown(),
                    },
                )?;

                let fs = LogicalFsReader::open(&evidence_dir, "evidence")
                    .map_err(|err| persistence_sqlite::DbError::System(err.to_string()))?;
                file_service::enumerate_filesystem(conn, &ds_id, &fs)?;

                let file_id = persistence_sqlite::repositories::file_repo::FileRepo::new(conn)
                    .find_by_data_source(&ds_id)?
                    .into_iter()
                    .find(|entry| entry.name == "note.txt")
                    .map(|entry| entry.id.0)
                    .expect("note.txt should be enumerated");
                let destination = tmp.path().join("exports").join("note-copy.txt");

                let result = extract_file_for_case(
                    conn,
                    &transport::commands::ExtractFileRequest {
                        file_id,
                        destination_path: destination.display().to_string(),
                        overwrite: false,
                    },
                )
                .map_err(|err| persistence_sqlite::DbError::System(err.message))?;

                assert!(result.contains("10 bytes"));
                assert_eq!(std::fs::read(&destination).unwrap(), b"extract me");

                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn oversized_media_preview_returns_scoped_handle_and_range_reads() {
        let oversized =
            vec![b'A'; infrastructure::constants::MAX_INLINE_MEDIA_PREVIEW_BYTES as usize + 1];
        with_logical_case_file(
            "large-media",
            "large.mp4",
            &oversized,
            |conn, file_id, _| {
                let state = test_state_with_case("case-media-large");
                let media = media_data_url_for_file(&state, conn, &file_id)
                    .map_err(|err| persistence_sqlite::DbError::System(err.message))?;
                assert_eq!(media.mode, MediaPreviewModeDto::Protocol);
                assert!(media
                    .url
                    .as_deref()
                    .is_some_and(|url| url.starts_with("evidence-media://handle/")));
                assert!(!media.url.as_deref().unwrap_or_default().contains(&file_id));
                assert!(!media.url.as_deref().unwrap_or_default().contains("file:"));
                let media_json = serde_json::to_string(&media)
                    .map_err(|err| persistence_sqlite::DbError::System(err.to_string()))?;
                assert!(!media_json.contains("large.mp4"));
                assert!(media.can_read_ranges);
                assert!(media.handle_id.is_some());
                assert_ne!(
                    media.handle_id.as_deref(),
                    Some(format!("file:{file_id}").as_str())
                );

                let range = media_range_for_file(
                    &state,
                    conn,
                    &MediaRangeRequestDto {
                        handle_id: media.handle_id.expect("handle"),
                        offset: 0,
                        length: 4,
                    },
                )
                .map_err(|err| persistence_sqlite::DbError::System(err.message))?;
                assert_eq!(range.bytes_read, 4);
                assert_eq!(range.bytes_base64, "QUFBQQ==");
                assert!(!range.eof);

                Ok(())
            },
        );
    }

    #[test]
    fn media_range_offset_at_size_returns_empty_eof() {
        with_logical_case_file(
            "media-eof",
            "clip.mp4",
            b"0123456789",
            |conn, file_id, _| {
                let state = test_state_with_case("case-media-eof");
                let handle_id = crate::media_protocol::create_scoped_media_handle(&state, &file_id)
                    .map_err(persistence_sqlite::DbError::System)?;
                let range = media_range_for_file(
                    &state,
                    conn,
                    &MediaRangeRequestDto {
                        handle_id,
                        offset: 10,
                        length: 8,
                    },
                )
                .map_err(|err| persistence_sqlite::DbError::System(err.message))?;

                assert_eq!(range.offset, 10);
                assert_eq!(range.bytes_base64, "");
                assert_eq!(range.bytes_read, 0);
                assert!(range.eof);

                Ok(())
            },
        );
    }

    #[test]
    fn media_range_rejects_invalid_handle() {
        with_logical_case_file(
            "media-invalid-handle",
            "clip.mp4",
            b"0123456789",
            |conn, _, _| {
                let state = test_state_with_case("case-media-invalid");
                let err = media_range_for_file(
                    &state,
                    conn,
                    &MediaRangeRequestDto {
                        handle_id: "C:/evidence/clip.mp4".to_string(),
                        offset: 0,
                        length: 8,
                    },
                )
                .expect_err("host paths must not be valid media handles");

                assert!(err.message.contains("media handle"));

                Ok(())
            },
        );
    }

    #[test]
    fn media_range_clamps_length_to_one_megabyte() {
        let content = vec![b'B'; MAX_VIEWER_RANGE_LENGTH as usize + 16];
        with_logical_case_file("media-clamp", "large.mp4", &content, |conn, file_id, _| {
            let state = test_state_with_case("case-media-clamp");
            let handle_id = crate::media_protocol::create_scoped_media_handle(&state, &file_id)
                .map_err(persistence_sqlite::DbError::System)?;
            let mut request = MediaRangeRequestDto {
                handle_id,
                offset: 0,
                length: u32::MAX,
            };
            request
                .validate()
                .map_err(persistence_sqlite::DbError::System)?;

            let range = media_range_for_file(&state, conn, &request)
                .map_err(|err| persistence_sqlite::DbError::System(err.message))?;

            assert_eq!(request.length, MAX_VIEWER_RANGE_LENGTH);
            assert_eq!(range.bytes_read, MAX_VIEWER_RANGE_LENGTH);
            assert!(!range.eof);

            Ok(())
        });
    }

    #[test]
    fn media_range_response_does_not_leak_host_path() {
        with_logical_case_file(
            "media-no-leak",
            "clip.mp4",
            b"0123456789",
            |conn, file_id, evidence_dir| {
                let state = test_state_with_case("case-media-no-leak");
                let handle_id = crate::media_protocol::create_scoped_media_handle(&state, &file_id)
                    .map_err(persistence_sqlite::DbError::System)?;
                let range = media_range_for_file(
                    &state,
                    conn,
                    &MediaRangeRequestDto {
                        handle_id,
                        offset: 2,
                        length: 4,
                    },
                )
                .map_err(|err| persistence_sqlite::DbError::System(err.message))?;
                let json = serde_json::to_string(&range)
                    .map_err(|err| persistence_sqlite::DbError::System(err.to_string()))?;

                assert!(!json.contains(&evidence_dir.to_string_lossy().to_string()));
                assert!(!json.contains("clip.mp4"));
                assert!(!json.contains(&file_id));

                Ok(())
            },
        );
    }
}
