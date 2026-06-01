//! File browsing and preview commands.
//!
//! Provides Tauri commands for:
//! - File tree navigation
//! - File listing and details
//! - File handle management for preview
//! - File range reading for hex/text viewer

use app_services::{file_service, text_service::TextService};
use base64::Engine;
use std::io::Read;
use tauri::State;
use transport::{
    commands::{ExtractFileRequest, GetFileChildrenRequest, GetFileRowsRequest},
    dto::{
        FileEntryRowDto, FileTreeNodeDto, ImagePreviewDto, MediaRangeRequestDto,
        MediaRangeResponseDto, MediaUrlDto, TextPreviewDto, ViewerHandleDto,
        ViewerRangeResponseDto,
    },
    CommandError,
};

#[cfg(test)]
use transport::dto::MAX_VIEWER_RANGE_LENGTH;

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
    request.validate().map_err(CommandError::invalid_input)?;
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
    request.validate().map_err(CommandError::invalid_input)?;
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
    mut request: transport::dto::ViewerRangeRequestDto,
) -> Result<ViewerRangeResponseDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
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

        if handle.size > infrastructure::constants::MAX_INLINE_IMAGE_PREVIEW_BYTES {
            return Err(CommandError::invalid_input(format!(
                "Image preview is limited to {} MB",
                infrastructure::constants::MAX_INLINE_IMAGE_PREVIEW_BYTES / (1024 * 1024)
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

        media_data_url_for_file(&conn, &file_id)
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

        media_range_for_file(&conn, &request)
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
    tauri::async_runtime::spawn_blocking(move || {
        let db_path = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            let active = guard.as_ref().ok_or_else(CommandError::no_active_case)?;
            active.db_path()
        };
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        extract_file_for_case(&conn, &request)
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
    if let Some(parent) = destination.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(CommandError::from_service_error)?;
        }
    }
    let mut output =
        std::fs::File::create(&destination).map_err(CommandError::from_service_error)?;
    let bytes =
        std::io::copy(&mut reader, &mut output).map_err(CommandError::from_service_error)?;
    Ok(format!("Extracted {} bytes", bytes))
}

fn media_data_url_for_file(
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
        return Ok(MediaUrlDto {
            url: None,
            handle_id: Some(handle.handle_id),
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
        url: Some(format!("data:{};base64,{}", mime, base64)),
        handle_id: Some(handle.handle_id),
        mime_type: mime,
        size: handle.size,
        can_read_ranges: true,
    })
}

fn media_range_for_file(
    conn: &rusqlite::Connection,
    request: &MediaRangeRequestDto,
) -> Result<MediaRangeResponseDto, CommandError> {
    let file_id = request
        .handle_id
        .strip_prefix("file:")
        .ok_or_else(|| CommandError::invalid_input("unsupported media handle"))?;
    let handle = file_service::open_file_handle_real(conn, file_id)
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
        file_service::open_file_content_by_id(conn, &domain::FileEntryId(file_id.to_string()))
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
                let media = media_data_url_for_file(conn, &file_id)
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
                let media = media_data_url_for_file(conn, &file_id)
                    .map_err(|err| persistence_sqlite::DbError::System(err.message))?;
                assert!(media.url.is_none());
                assert!(media.can_read_ranges);
                assert_eq!(
                    media.handle_id.as_deref(),
                    Some(format!("file:{file_id}").as_str())
                );

                let range = media_range_for_file(
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
                let range = media_range_for_file(
                    conn,
                    &MediaRangeRequestDto {
                        handle_id: format!("file:{file_id}"),
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
                let err = media_range_for_file(
                    conn,
                    &MediaRangeRequestDto {
                        handle_id: "C:/evidence/clip.mp4".to_string(),
                        offset: 0,
                        length: 8,
                    },
                )
                .expect_err("host paths must not be valid media handles");

                assert!(err.message.contains("unsupported media handle"));

                Ok(())
            },
        );
    }

    #[test]
    fn media_range_clamps_length_to_one_megabyte() {
        let content = vec![b'B'; MAX_VIEWER_RANGE_LENGTH as usize + 16];
        with_logical_case_file("media-clamp", "large.mp4", &content, |conn, file_id, _| {
            let mut request = MediaRangeRequestDto {
                handle_id: format!("file:{file_id}"),
                offset: 0,
                length: u32::MAX,
            };
            request
                .validate()
                .map_err(persistence_sqlite::DbError::System)?;

            let range = media_range_for_file(conn, &request)
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
                let range = media_range_for_file(
                    conn,
                    &MediaRangeRequestDto {
                        handle_id: format!("file:{file_id}"),
                        offset: 2,
                        length: 4,
                    },
                )
                .map_err(|err| persistence_sqlite::DbError::System(err.message))?;
                let json = serde_json::to_string(&range)
                    .map_err(|err| persistence_sqlite::DbError::System(err.to_string()))?;

                assert!(!json.contains(&evidence_dir.to_string_lossy().to_string()));
                assert!(!json.contains("clip.mp4"));
                assert!(!json.contains("file:"));

                Ok(())
            },
        );
    }
}
