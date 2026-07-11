//! File browsing and preview commands.
//!
//! Provides Tauri commands for:
//! - File tree navigation
//! - File listing and details
//! - File handle management for preview
//! - File range reading for hex/text viewer

use app_services::file_service::{self, MediaPreviewPlan};
use persistence_sqlite::repositories::audit_repo::AuditAction;
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

use super::command_support::write_audit_log;
use crate::state::AppState;

#[cfg(test)]
type PreviewReadCounter =
    std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<String, usize>>>;

#[cfg(test)]
static MEDIA_BYTES_HELPER_CALLS: PreviewReadCounter =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

#[cfg(test)]
fn increment_preview_read_counter(counter: &PreviewReadCounter, case_id: &str) {
    let Ok(mut counts) = counter.lock() else {
        return;
    };
    *counts.entry(case_id.to_string()).or_insert(0) += 1;
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
        let active = crate::commands::command_support::require_active_case(&app_state)?;
        file_service::get_file_children_for_case(
            &conn,
            &active.case_root,
            &active.meta.id,
            &request.parent_id,
            request.offset,
            request.limit,
            request.show_hidden,
        )
        .map_err(CommandError::from_typed_service_error)
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
        let active = crate::commands::command_support::require_active_case(&app_state)?;
        file_service::get_file_tree_for_case(
            &conn,
            &active.case_root,
            &active.meta.id,
            request.show_hidden,
        )
        .map_err(CommandError::from_typed_service_error)
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
        let active = crate::commands::command_support::require_active_case(&app_state)?;
        file_service::get_file_rows_for_case(&conn, &active.case_root, &active.meta.id, &request)
            .map_err(CommandError::from_typed_service_error)
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
        let active = crate::commands::command_support::require_active_case(&app_state)?;
        file_service::get_file_jump_context_for_case(
            &conn,
            &active.case_root,
            &active.meta.id,
            &request,
        )
        .map_err(|err| {
            if err.to_string().contains("not found") {
                CommandError::not_found("File")
            } else {
                CommandError::from_typed_service_error(err)
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
        let active = crate::commands::command_support::require_active_case(&app_state)?;
        file_service::open_file_handle_for_case(&conn, &active.case_root, &active.meta.id, &file_id)
            .map_err(CommandError::from_typed_service_error)
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
        let active = crate::commands::command_support::require_active_case(&app_state)?;
        file_service::open_file_handle_for_case(
            &conn,
            &active.case_root,
            &active.meta.id,
            &request.file_id,
        )
        .map_err(CommandError::from_typed_service_error)
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
    tauri::async_runtime::spawn_blocking(move || read_file_range_for_state(&app_state, &request))
        .await
        .map_err(CommandError::from_join_error)?
}

fn read_file_range_for_state(
    app_state: &AppState,
    request: &transport::dto::ViewerRangeRequestDto,
) -> Result<ViewerRangeResponseDto, CommandError> {
    let conn = crate::commands::command_support::get_case_connection(app_state)?;
    let active = crate::commands::command_support::require_active_case(app_state)?;
    file_service::read_file_range_for_source_case(
        &conn,
        &active.case_root,
        &active.meta.id,
        request,
    )
    .map_err(CommandError::from_typed_service_error)
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
        let conn = crate::commands::command_support::get_case_connection(&app_state)?;
        text_preview_for_file(&app_state, &conn, &file_id, max_bytes)
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
        image_preview_for_file(&app_state, &conn, &file_id)
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
    let (audit_file_id, audit_dest, overwrite, file_id, dest) = (
        request.file_id.clone(),
        request.destination_path.clone(),
        request.overwrite,
        request.file_id.clone(),
        std::path::PathBuf::from(&request.destination_path),
    );
    tauri::async_runtime::spawn_blocking(move || {
        let conn = crate::commands::command_support::get_case_connection(&app_state)?;
        let active = crate::commands::command_support::require_active_case(&app_state)?;
        let outcome: Result<String, CommandError> =
            file_service::extract_file_to_destination_for_case(
                &conn,
                &active.case_root,
                &active.meta.id,
                &file_id,
                &dest,
                overwrite,
            )
                .map(|w| format!("Extracted {} bytes", w))
                .map_err(CommandError::from_typed_service_error);
        let dest_file_name = std::path::Path::new(&audit_dest)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        match &outcome {
            Ok(msg) => write_audit_log(
                &app_state,
                AuditAction::FileExtract,
                Some(&audit_file_id),
                serde_json::json!({ "status":"ok","overwrite":overwrite,"destinationFileName":dest_file_name,"message":msg }),
            ),
            Err(err) => write_audit_log(
                &app_state,
                AuditAction::FileExtract,
                Some(&audit_file_id),
                serde_json::json!({ "status":"failed","overwrite":overwrite,"destinationFileName":dest_file_name,"errorCode":err.code,"errorCategory":err.category }),
            ),
        }
        outcome
    })
    .await
    .map_err(CommandError::from_join_error)?
}

fn image_preview_for_file(
    state: &AppState,
    conn: &rusqlite::Connection,
    file_id: &str,
) -> Result<ImagePreviewDto, CommandError> {
    let active = crate::commands::command_support::require_active_case(state)?;
    file_service::image_preview_for_source_case(conn, &active.case_root, &active.meta.id, file_id)
        .map_err(CommandError::from_typed_service_error)
}

fn media_data_url_for_file(
    state: &AppState,
    conn: &rusqlite::Connection,
    file_id: &str,
) -> Result<MediaUrlDto, CommandError> {
    let active = crate::commands::command_support::require_active_case(state)?;
    let plan = file_service::media_preview_plan_for_source_case(
        conn,
        &active.case_root,
        &active.meta.id,
        file_id,
    )
    .map_err(CommandError::from_typed_service_error)?;

    match plan {
        MediaPreviewPlan::Inline(dto) => Ok(dto),
        MediaPreviewPlan::Protocol {
            mime_type,
            size,
            can_read_ranges,
        } => {
            let scoped_handle = crate::media_protocol::create_scoped_media_handle(state, file_id)
                .map_err(CommandError::security)?;
            Ok(MediaUrlDto {
                mode: MediaPreviewModeDto::Protocol,
                url: Some(crate::media_protocol::media_protocol_url(&scoped_handle)),
                handle_id: Some(scoped_handle),
                mime_type,
                size,
                can_read_ranges,
            })
        }
    }
}

fn media_range_for_file(
    state: &AppState,
    conn: &rusqlite::Connection,
    request: &MediaRangeRequestDto,
) -> Result<MediaRangeResponseDto, CommandError> {
    let file_id = crate::media_protocol::resolve_scoped_media_handle(state, &request.handle_id)
        .map_err(CommandError::security)?;
    #[cfg(test)]
    {
        let case_id = crate::commands::command_support::snapshot_active_case(state)?
            .map(|active| active.case_id)
            .ok_or_else(CommandError::no_active_case)?;
        increment_preview_read_counter(&MEDIA_BYTES_HELPER_CALLS, &case_id);
    }

    let active = crate::commands::command_support::require_active_case(state)?;
    file_service::media_range_for_source_case(
        conn,
        &active.case_root,
        &active.meta.id,
        &file_id,
        request,
    )
    .map_err(CommandError::from_typed_service_error)
}

fn text_preview_for_file(
    state: &AppState,
    conn: &rusqlite::Connection,
    file_id: &str,
    max_bytes: Option<usize>,
) -> Result<TextPreviewDto, CommandError> {
    let active = crate::commands::command_support::require_active_case(state)?;
    file_service::text_preview_for_source_case(
        conn,
        &active.case_root,
        &active.meta.id,
        file_id,
        max_bytes,
    )
    .map_err(CommandError::from_typed_service_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_services::{case_service, file_service};
    use base64::Engine;
    use evidence_core::LogicalFsReader;
    use persistence_sqlite::repositories::{
        datasource_repo::{DataSourceRepo, DataSourceStorage},
        file_repo::FileRepo,
    };
    use tempfile::TempDir;

    fn preview_counter_value(counter: &PreviewReadCounter, case_id: &str) -> usize {
        counter
            .lock()
            .ok()
            .and_then(|counts| counts.get(case_id).copied())
            .unwrap_or(0)
    }

    fn media_bytes_helper_call_count(case_id: &str) -> usize {
        preview_counter_value(&MEDIA_BYTES_HELPER_CALLS, case_id)
    }

    fn test_state_with_case(case_id: &str, case_root: impl Into<std::path::PathBuf>) -> AppState {
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
            case_root.into(),
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
            String,
            std::path::PathBuf,
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
        let case_root = active.case_root.clone();

        active
            .with_conn(|conn| {
                let ds_id = domain::DataSourceId("ds-media".to_string());
                let ds = domain::DataSource {
                    id: ds_id.clone(),
                    name: "evidence".to_string(),
                    kind: domain::DataSourceKind::LogicalDirectory,
                    source_path: evidence_dir.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                };
                let mut storage = DataSourceStorage::source_db(&ds_id.0, Some("windows"), None);
                storage.import_state = "ready".to_string();
                DataSourceRepo::new(conn).insert_with_storage(&case_id, &ds, &storage)?;

                let source_conn = app_services::source_db::open_source_db(&case_root, &ds_id)?;
                DataSourceRepo::new(&source_conn).upsert_source_local_metadata(&case_id, &ds)?;

                let fs = LogicalFsReader::open(&evidence_dir, "evidence")
                    .map_err(|err| persistence_sqlite::DbError::System(err.to_string()))?;
                file_service::enumerate_filesystem(&source_conn, &ds_id, &fs)?;

                let local_file_id =
                    persistence_sqlite::repositories::file_repo::FileRepo::new(&source_conn)
                        .find_by_data_source(&ds_id)?
                        .into_iter()
                        .find(|entry| entry.name == file_name)
                        .map(|entry| entry.id.0)
                        .expect("file should be enumerated");
                let file_id = app_services::source_db::GlobalFileId::new(
                    ds_id,
                    domain::FileEntryId(local_file_id),
                )
                .encode()
                .0;

                test(
                    conn,
                    case_id.0.clone(),
                    file_id,
                    evidence_dir.clone(),
                    case_root.clone(),
                )?;
                Ok(())
            })
            .unwrap();
    }

    fn with_raw_exfat_case_file(
        case_name: &str,
        ext: &str,
        test: impl FnOnce(
            &rusqlite::Connection,
            String,
            String,
            std::path::PathBuf,
        ) -> Result<(), persistence_sqlite::DbError>,
    ) {
        let tmp = TempDir::new().unwrap();
        let raw_path = tmp.path().join("exfat.raw");
        write_exfat_raw_fixture(&raw_path).unwrap();

        let active =
            case_service::create_case(&tmp.path().join("cases"), case_name, Some("tester"))
                .unwrap();
        let case_id = active.meta.id.clone();
        let case_root = active.case_root.clone();

        active
            .with_conn(|conn| {
                let ds_id = domain::DataSourceId("ds-raw-exfat-media".to_string());
                let ds = domain::DataSource {
                    id: ds_id.clone(),
                    name: "raw exfat evidence".to_string(),
                    kind: domain::DataSourceKind::Raw,
                    source_path: raw_path,
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                };
                let mut storage = DataSourceStorage::source_db(&ds_id.0, Some("windows"), None);
                storage.import_state = "ready".to_string();
                DataSourceRepo::new(conn).insert_with_storage(&case_id, &ds, &storage)?;
                let source_conn = app_services::source_db::open_source_db(&case_root, &ds_id)?;
                DataSourceRepo::new(&source_conn).upsert_source_local_metadata(&case_id, &ds)?;

                let file_id = domain::FileEntryId("file-raw-exfat-large".to_string());
                FileRepo::new(&source_conn).insert_batch(&[domain::FileEntry {
                    id: file_id.clone(),
                    parent_id: None,
                    data_source_id: ds_id.clone(),
                    path: "LARGE.BIN".to_string(),
                    name: "LARGE.BIN".to_string(),
                    entry_type: domain::EntryType::File,
                    size: Some(1536),
                    ext: Some(ext.to_string()),
                    deleted: false,
                    hidden: false,
                    system: false,
                    encrypted: false,
                    created_at: None,
                    modified_at: None,
                    accessed_at: None,
                    changed_at: None,
                    hash_sha256: None,
                }])?;
                let global_file_id = app_services::source_db::GlobalFileId::new(ds_id, file_id)
                    .encode()
                    .0;

                test(conn, case_id.0.clone(), global_file_id, case_root.clone())
            })
            .unwrap();
    }

    fn write_exfat_raw_fixture(path: &std::path::Path) -> std::io::Result<()> {
        const SECTOR_SIZE: usize = 512;
        const FAT_SECTOR: usize = 24;
        const CLUSTER_HEAP_SECTOR: usize = 32;
        const CLUSTER_SIZE: usize = SECTOR_SIZE;
        const FILE_SIZE: usize = CLUSTER_SIZE * 3;
        const TOTAL_SECTORS: usize = 1024;

        let mut data = vec![0u8; TOTAL_SECTORS * SECTOR_SIZE];

        let boot = &mut data[0..SECTOR_SIZE];
        boot[0..3].copy_from_slice(&[0xEB, 0x76, 0x90]);
        boot[3..11].copy_from_slice(b"EXFAT   ");
        boot[72..80].copy_from_slice(&(TOTAL_SECTORS as u64).to_le_bytes());
        boot[80..84].copy_from_slice(&(FAT_SECTOR as u32).to_le_bytes());
        boot[84..88].copy_from_slice(&1u32.to_le_bytes());
        boot[88..92].copy_from_slice(&(CLUSTER_HEAP_SECTOR as u32).to_le_bytes());
        boot[92..96].copy_from_slice(&100u32.to_le_bytes());
        boot[96..100].copy_from_slice(&2u32.to_le_bytes());
        boot[100..104].copy_from_slice(&0x12345678u32.to_le_bytes());
        boot[104..106].copy_from_slice(&0x0100u16.to_le_bytes());
        boot[108] = 9;
        boot[109] = 0;
        boot[110] = 1;
        boot[111] = 0x80;
        boot[112] = 0xFF;
        boot[510..512].copy_from_slice(&0xAA55u16.to_le_bytes());

        let fat_offset = FAT_SECTOR * SECTOR_SIZE;
        let fat = &mut data[fat_offset..fat_offset + SECTOR_SIZE];
        fat[0..4].copy_from_slice(&[0xF8, 0xFF, 0xFF, 0xFF]);
        fat[4..8].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
        fat[8..12].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
        fat[12..16].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);

        let root_offset = CLUSTER_HEAP_SECTOR * SECTOR_SIZE;
        let root = &mut data[root_offset..root_offset + CLUSTER_SIZE];
        let mut pos = 0usize;

        root[pos] = 0x85;
        root[pos + 1] = 0x02;
        root[pos + 4..pos + 6].copy_from_slice(&0x20u16.to_le_bytes());
        pos += 32;

        root[pos] = 0xC0;
        root[pos + 1] = 0x02;
        root[pos + 3] = "LARGE.BIN".encode_utf16().count() as u8;
        root[pos + 8..pos + 16].copy_from_slice(&(FILE_SIZE as u64).to_le_bytes());
        root[pos + 20..pos + 24].copy_from_slice(&3u32.to_le_bytes());
        root[pos + 24..pos + 32].copy_from_slice(&(FILE_SIZE as u64).to_le_bytes());
        pos += 32;

        root[pos] = 0xC1;
        for (i, ch) in "LARGE.BIN".encode_utf16().enumerate() {
            let offset = pos + 2 + i * 2;
            root[offset..offset + 2].copy_from_slice(&ch.to_le_bytes());
        }

        for cluster in 3..=5usize {
            let value = match cluster {
                3 => b'A',
                4 => b'B',
                5 => b'C',
                _ => unreachable!(),
            };
            let offset = CLUSTER_HEAP_SECTOR * SECTOR_SIZE + (cluster - 2) * CLUSTER_SIZE;
            data[offset..offset + CLUSTER_SIZE].fill(value);
        }

        std::fs::write(path, data)
    }

    #[test]
    fn read_file_range_requires_active_case_instead_of_empty_hex_fallback() {
        let state = AppState::default();
        let request = transport::dto::ViewerRangeRequestDto {
            handle_id: "file:any".to_string(),
            offset: 0,
            length: 16,
        };

        let err = read_file_range_for_state(&state, &request)
            .expect_err("active case should be required");

        assert_eq!(err.code, "NO_ACTIVE_CASE");
        assert!(err.message.contains("No active case"));
    }

    #[test]
    fn media_preview_returns_data_url_without_host_path() {
        with_logical_case_file(
            "media",
            "clip.mp4",
            b"tiny media bytes",
            |conn, case_id, file_id, evidence_dir, case_root| {
                let state = test_state_with_case(&case_id, case_root);
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
    fn media_preview_logical_directory_reads_direct_without_service_fallback() {
        with_logical_case_file(
            "media-inline-logical",
            "clip.mp4",
            b"tiny media bytes",
            |conn, case_id, file_id, _, case_root| {
                let state = test_state_with_case(&case_id, case_root);
                let media = media_data_url_for_file(&state, conn, &file_id)
                    .map_err(|err| persistence_sqlite::DbError::System(err.message))?;

                assert_eq!(media.mode, MediaPreviewModeDto::Inline);
                let (_, encoded) = media
                    .url
                    .as_deref()
                    .expect("small media should return inline URL")
                    .split_once(',')
                    .expect("data URL payload");
                assert_eq!(
                    base64::engine::general_purpose::STANDARD
                        .decode(encoded.as_bytes())
                        .unwrap(),
                    b"tiny media bytes"
                );

                Ok(())
            },
        );
    }

    #[test]
    fn media_preview_raw_image_reads_via_bytes_only_service_path() {
        with_raw_exfat_case_file(
            "media-raw-inline",
            "mp4",
            |conn, case_id, file_id, case_root| {
                let state = test_state_with_case(&case_id, case_root);
                let media = media_data_url_for_file(&state, conn, &file_id)
                    .map_err(|err| persistence_sqlite::DbError::System(err.message))?;

                assert_eq!(media.mode, MediaPreviewModeDto::Inline);
                let (_, encoded) = media
                    .url
                    .as_deref()
                    .expect("small media should return inline URL")
                    .split_once(',')
                    .expect("data URL payload");
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(encoded.as_bytes())
                    .unwrap();
                assert_eq!(decoded.len(), 1536);
                assert_eq!(&decoded[0..512], vec![b'A'; 512].as_slice());
                assert_eq!(&decoded[512..1024], vec![b'B'; 512].as_slice());
                assert_eq!(&decoded[1024..1536], vec![b'C'; 512].as_slice());

                Ok(())
            },
        );
    }

    #[test]
    fn image_preview_logical_directory_reads_direct_without_service_fallback() {
        with_logical_case_file(
            "image-inline-logical",
            "tiny.png",
            b"tiny image bytes",
            |conn, case_id, file_id, _, case_root| {
                let state = test_state_with_case(&case_id, case_root);
                let image = image_preview_for_file(&state, conn, &file_id)
                    .map_err(|err| persistence_sqlite::DbError::System(err.message))?;

                assert_eq!(image.mime_type, "image/png");
                let (_, encoded) = image.data_url.split_once(',').expect("data URL payload");
                assert_eq!(
                    base64::engine::general_purpose::STANDARD
                        .decode(encoded.as_bytes())
                        .unwrap(),
                    b"tiny image bytes"
                );

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
                let ds = domain::DataSource {
                    id: ds_id.clone(),
                    name: "evidence".to_string(),
                    kind: domain::DataSourceKind::LogicalDirectory,
                    source_path: evidence_dir.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                };
                let mut storage = DataSourceStorage::source_db(&ds_id.0, Some("windows"), None);
                storage.import_state = "ready".to_string();
                DataSourceRepo::new(conn).insert_with_storage(&case_id, &ds, &storage)?;
                let source_conn =
                    app_services::source_db::open_source_db(&active.case_root, &ds_id)?;
                DataSourceRepo::new(&source_conn).upsert_source_local_metadata(&case_id, &ds)?;

                let fs = LogicalFsReader::open(&evidence_dir, "evidence")
                    .map_err(|err| persistence_sqlite::DbError::System(err.to_string()))?;
                file_service::enumerate_filesystem(&source_conn, &ds_id, &fs)?;

                let local_file_id =
                    persistence_sqlite::repositories::file_repo::FileRepo::new(&source_conn)
                        .find_by_data_source(&ds_id)?
                        .into_iter()
                        .find(|entry| entry.name == "note.txt")
                        .map(|entry| entry.id.0)
                        .expect("note.txt should be enumerated");
                let file_id = app_services::source_db::GlobalFileId::new(
                    ds_id,
                    domain::FileEntryId(local_file_id),
                )
                .encode()
                .0;
                let destination = tmp.path().join("exports").join("note-copy.txt");
                let written = file_service::extract_file_to_destination_for_case(
                    conn,
                    &active.case_root,
                    &active.meta.id,
                    &file_id,
                    &destination,
                    false,
                )
                .map_err(|err| persistence_sqlite::DbError::System(err.to_string()))?;

                assert_eq!(written, 10);
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
            |conn, case_id, file_id, _, case_root| {
                let state = test_state_with_case(&case_id, case_root);
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
            |conn, case_id, file_id, _, case_root| {
                let state = test_state_with_case(&case_id, case_root);
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
    fn media_range_mid_file_reads_raw_bytes_without_hex_viewer_path() {
        let content: Vec<u8> = (0u8..64).collect();
        with_logical_case_file(
            "media-mid-range",
            "clip.mp4",
            &content,
            |conn, case_id, file_id, _, case_root| {
                let state = test_state_with_case(&case_id, case_root);
                let handle_id = crate::media_protocol::create_scoped_media_handle(&state, &file_id)
                    .map_err(persistence_sqlite::DbError::System)?;
                let media_helper_before = media_bytes_helper_call_count(&case_id);
                let range = media_range_for_file(
                    &state,
                    conn,
                    &MediaRangeRequestDto {
                        handle_id,
                        offset: 17,
                        length: 12,
                    },
                )
                .map_err(|err| persistence_sqlite::DbError::System(err.message))?;

                assert_eq!(range.offset, 17);
                assert_eq!(range.bytes_read, 12);
                assert_eq!(
                    media_bytes_helper_call_count(&case_id) - media_helper_before,
                    1
                );
                assert_eq!(
                    base64::engine::general_purpose::STANDARD
                        .decode(range.bytes_base64.as_bytes())
                        .unwrap(),
                    content[17..29].to_vec()
                );
                assert!(!range.eof);

                Ok(())
            },
        );
    }

    #[test]
    fn media_range_mid_raw_image_reads_via_bytes_only_service_path() {
        with_raw_exfat_case_file(
            "media-raw-range",
            "bin",
            |conn, case_id, file_id, case_root| {
                let state = test_state_with_case(&case_id, case_root);
                let handle_id = crate::media_protocol::create_scoped_media_handle(&state, &file_id)
                    .map_err(persistence_sqlite::DbError::System)?;

                let media_helper_before = media_bytes_helper_call_count(&case_id);
                let range = media_range_for_file(
                    &state,
                    conn,
                    &MediaRangeRequestDto {
                        handle_id,
                        offset: 512 + 7,
                        length: 9,
                    },
                )
                .map_err(|err| persistence_sqlite::DbError::System(err.message))?;

                assert_eq!(range.offset, 512 + 7);
                assert_eq!(range.bytes_read, 9);
                assert_eq!(
                    media_bytes_helper_call_count(&case_id) - media_helper_before,
                    1
                );
                assert_eq!(
                    base64::engine::general_purpose::STANDARD
                        .decode(range.bytes_base64.as_bytes())
                        .unwrap(),
                    vec![b'B'; 9]
                );
                assert!(!range.eof);

                Ok(())
            },
        );
    }

    #[test]
    fn image_preview_raw_image_reads_via_bytes_only_service_path() {
        with_raw_exfat_case_file(
            "image-raw-inline",
            "png",
            |conn, case_id, file_id, case_root| {
                let state = test_state_with_case(&case_id, case_root);

                let image = image_preview_for_file(&state, conn, &file_id)
                    .map_err(|err| persistence_sqlite::DbError::System(err.message))?;

                assert_eq!(image.mime_type, "image/png");
                let (_, encoded) = image.data_url.split_once(',').expect("data URL payload");
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(encoded.as_bytes())
                    .unwrap();
                assert_eq!(decoded.len(), 1536);
                assert_eq!(&decoded[0..512], vec![b'A'; 512].as_slice());
                assert_eq!(&decoded[512..1024], vec![b'B'; 512].as_slice());
                assert_eq!(&decoded[1024..1536], vec![b'C'; 512].as_slice());

                Ok(())
            },
        );
    }

    #[test]
    fn text_preview_raw_image_header_reads_via_bytes_only_service_path() {
        with_raw_exfat_case_file(
            "text-raw-header",
            "bin",
            |conn, case_id, file_id, case_root| {
                let state = test_state_with_case(&case_id, case_root);

                let preview = text_preview_for_file(&state, conn, &file_id, Some(16))
                    .map_err(|err| persistence_sqlite::DbError::System(err.message))?;

                assert_eq!(preview.content, "AAAAAAAAAAAAAAAA");
                assert_eq!(preview.encoding, "UTF-8");
                assert!(!preview.is_binary);
                assert!(preview.is_truncated);

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
            |conn, case_id, _, _, case_root| {
                let state = test_state_with_case(&case_id, case_root);
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
        with_logical_case_file(
            "media-clamp",
            "large.mp4",
            &content,
            |conn, case_id, file_id, _, case_root| {
                let state = test_state_with_case(&case_id, case_root);
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
            },
        );
    }

    #[test]
    fn media_range_response_does_not_leak_host_path() {
        with_logical_case_file(
            "media-no-leak",
            "clip.mp4",
            b"0123456789",
            |conn, case_id, file_id, evidence_dir, case_root| {
                let state = test_state_with_case(&case_id, case_root);
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
