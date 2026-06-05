use app_services::case_service;
use domain::{
    DataSource, DataSourceId, DataSourceKind, DataSourceProvenance, EntryType, FileEntry,
    FileEntryId,
};
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, file_repo::FileRepo};
use std::path::PathBuf;
use tauri::{AppHandle, State};
use transport::{
    commands::{
        CreateCaseRequest, DeleteCaseRequest, DeleteDataSourceRequest, OpenCaseRequest,
        RenameDataSourceRequest,
    },
    dto::{CaseMetricsDto, CaseSummaryDto, DataSourceSummaryDto, RecentCaseDto, RecentObjectDto},
    CommandError,
};
use uuid::Uuid;

use crate::{events::event_bridge, state::AppState};

fn meta_to_dto(meta: &domain::CaseMeta) -> CaseSummaryDto {
    CaseSummaryDto {
        id: meta.id.0.clone(),
        name: meta.name.clone(),
        number: meta.number.clone(),
        examiner: meta.examiner.clone(),
        created_at: meta.created_at.to_rfc3339(),
        updated_at: meta.updated_at.to_rfc3339(),
    }
}

#[tauri::command]
pub fn create_case(
    state: State<AppState>,
    app: AppHandle,
    request: CreateCaseRequest,
) -> Result<CaseSummaryDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let root = PathBuf::from(&request.case_root);
    let active = case_service::create_case(&root, &request.name, request.examiner.as_deref())
        .map_err(CommandError::from_service_error)?;

    let dto = meta_to_dto(&active.meta);
    let mut guard = state
        .active_case
        .lock()
        .map_err(|e| CommandError::from_lock_error("Case", e))?;
    *guard = Some(active);
    remember_recent_case(&root, &dto)?;
    event_bridge::emit_case_opened(&app, &dto.id, &dto.name);
    Ok(dto)
}

#[tauri::command]
pub fn open_case(
    state: State<AppState>,
    app: AppHandle,
    request: OpenCaseRequest,
) -> Result<CaseSummaryDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let root = PathBuf::from(&request.case_root);
    let active = case_service::open_case(&root).map_err(CommandError::from_service_error)?;

    let dto = meta_to_dto(&active.meta);
    let mut guard = state
        .active_case
        .lock()
        .map_err(|e| CommandError::from_lock_error("Case", e))?;
    *guard = Some(active);
    remember_recent_case(&root, &dto)?;
    event_bridge::emit_case_opened(&app, &dto.id, &dto.name);
    Ok(dto)
}

#[tauri::command]
pub fn create_analysis_demo_case(
    state: State<AppState>,
    app: AppHandle,
) -> Result<CaseSummaryDto, CommandError> {
    let case_root = std::env::temp_dir().join("forensics-workbench-analysis-demo");
    if case_root.exists() {
        std::fs::remove_dir_all(&case_root).map_err(|e| {
            CommandError::internal(format!("Failed to reset analysis demo case: {e}"))
        })?;
    }
    std::fs::create_dir_all(&case_root)
        .map_err(|e| CommandError::internal(format!("Failed to create analysis demo root: {e}")))?;

    let active = case_service::create_case(&case_root, "Analysis Demo", Some("Codex Demo"))
        .map_err(CommandError::from_service_error)?;
    seed_analysis_demo(&active).map_err(CommandError::from_service_error)?;

    let dto = meta_to_dto(&active.meta);
    let mut guard = state
        .active_case
        .lock()
        .map_err(|e| CommandError::from_lock_error("Case", e))?;
    *guard = Some(active);
    remember_recent_case(&case_root.join("Analysis Demo"), &dto)?;
    event_bridge::emit_case_opened(&app, &dto.id, &dto.name);
    Ok(dto)
}

#[tauri::command]
pub fn get_current_case(state: State<AppState>) -> Result<Option<CaseSummaryDto>, CommandError> {
    let guard = state
        .active_case
        .lock()
        .map_err(|e| CommandError::from_lock_error("Case", e))?;
    match guard.as_ref() {
        Some(active) => Ok(Some(meta_to_dto(&active.meta))),
        None => Ok(None),
    }
}

#[tauri::command]
pub fn close_case(state: State<AppState>, app: AppHandle) -> Result<(), CommandError> {
    // 1. Cancel all background tasks
    state.task_manager.cancel_all();

    // 2. Wait for tasks to complete (with timeout)
    let _ = state
        .task_manager
        .wait_all(std::time::Duration::from_secs(5));

    // 3. Clear active case
    let mut guard = state
        .active_case
        .lock()
        .map_err(|e| CommandError::from_lock_error("Case", e))?;
    let closed_case_id = guard.as_ref().map(|active| active.meta.id.0.clone());
    *guard = None;
    if let Some(case_id) = closed_case_id {
        event_bridge::emit_case_closed(&app, &case_id);
    }
    Ok(())
}

#[tauri::command]
pub async fn get_case_metrics(state: State<'_, AppState>) -> Result<CaseMetricsDto, CommandError> {
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
                None => {
                    return Ok(CaseMetricsDto {
                        data_source_count: 0,
                        indexed_file_count: 0,
                        timeline_event_count: 0,
                        artifact_count: 0,
                    })
                }
            }
        };
        // Guard is now dropped — query with a fresh connection
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        let repo = persistence_sqlite::repositories::case_repo::CaseRepo::new(&conn);
        let metrics = repo
            .get_metrics()
            .map_err(CommandError::from_service_error)?;
        Ok(CaseMetricsDto {
            data_source_count: metrics.data_source_count,
            indexed_file_count: metrics.indexed_file_count,
            timeline_event_count: metrics.timeline_event_count,
            artifact_count: metrics.artifact_count,
        })
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn get_recent_objects(
    state: State<'_, AppState>,
) -> Result<Vec<RecentObjectDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
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
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        app_services::file_service::get_recent_objects_real(&conn)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn get_data_sources(
    state: State<'_, AppState>,
) -> Result<Vec<DataSourceSummaryDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (db_path, case_id) = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => (active.db_path(), active.meta.id.clone()),
                None => return Ok(vec![]),
            }
        };
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        app_services::file_service::get_data_sources_real(&conn, &case_id)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn rename_data_source(
    state: State<'_, AppState>,
    request: RenameDataSourceRequest,
) -> Result<(), CommandError> {
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
        app_services::file_service::rename_data_source_real(
            &conn,
            &request.data_source_id,
            &request.name,
        )
        .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub fn get_recent_cases() -> Result<Vec<RecentCaseDto>, CommandError> {
    read_recent_cases()
}

#[tauri::command]
pub fn remove_case_from_list(request: DeleteCaseRequest) -> Result<String, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let mut recent = read_recent_cases().unwrap_or_else(|e| {
        tracing::warn!("Failed to read recent cases, starting fresh: {}", e);
        Vec::new()
    });
    recent.retain(|item| item.case_root != request.case_root);
    save_recent_cases(&recent)?;
    Ok(format!("Removed from list: {}", request.case_root))
}

#[tauri::command]
pub async fn delete_case(
    state: State<'_, AppState>,
    request: DeleteCaseRequest,
) -> Result<String, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let root = PathBuf::from(&request.case_root);

    {
        let mut guard = state
            .active_case
            .lock()
            .map_err(|e| CommandError::from_lock_error("Case", e))?;
        if let Some(ref active) = *guard {
            if active.case_root == root {
                *guard = None;
            }
        }
    }

    let root_clone = root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        case_service::delete_case(&root_clone).map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)??;

    let mut recent = read_recent_cases().unwrap_or_else(|e| {
        tracing::warn!("Failed to read recent cases, starting fresh: {}", e);
        Vec::new()
    });
    recent.retain(|item| item.case_root != request.case_root);
    save_recent_cases(&recent)?;

    Ok("Case deleted".to_string())
}

#[tauri::command]
pub async fn delete_data_source(
    state: State<'_, AppState>,
    request: DeleteDataSourceRequest,
) -> Result<String, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let ds_id = request.data_source_id.clone();
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
        case_service::delete_data_source(&conn, &ds_id)
            .map_err(CommandError::from_service_error)?;
        Ok("Data source deleted".to_string())
    })
    .await
    .map_err(CommandError::from_join_error)?
}

const RECENT_CASES_FILE: &str = "forensics-recent-cases.json";
const MAX_RECENT_CASES: usize = 8;

fn recent_cases_path() -> Result<PathBuf, CommandError> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .ok_or_else(|| CommandError::internal("Cannot resolve APPDATA for recent cases"))?;
    Ok(base.join("ForensicsWorkbench").join(RECENT_CASES_FILE))
}

fn save_recent_cases(recent: &[RecentCaseDto]) -> Result<(), CommandError> {
    let path = recent_cases_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            tracing::error!("Failed to create recent cases directory: {}", e);
            CommandError::internal("Failed to save recent cases")
        })?;
    }
    let json = serde_json::to_string_pretty(recent).map_err(|e| {
        tracing::error!("Failed to serialize recent cases: {}", e);
        CommandError::internal("Failed to save recent cases")
    })?;
    std::fs::write(&path, json).map_err(|e| {
        tracing::error!("Failed to write recent cases file: {}", e);
        CommandError::internal("Failed to save recent cases")
    })?;

    // Set restrictive permissions (Windows: current user only)
    #[cfg(target_os = "windows")]
    {
        // On Windows, we rely on NTFS permissions inherited from %APPDATA%
        // which is already user-specific. Log if we can't verify.
        tracing::debug!("Recent cases file saved to user-specific APPDATA directory");
    }

    // Set restrictive permissions (Unix: 0o600)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)) {
            tracing::warn!(
                "Failed to set restrictive permissions on recent cases file: {}",
                e
            );
        }
    }

    Ok(())
}

fn remember_recent_case(
    case_root: &std::path::Path,
    summary: &CaseSummaryDto,
) -> Result<(), CommandError> {
    let mut recent = read_recent_cases().unwrap_or_else(|e| {
        tracing::warn!("Failed to read recent cases, starting fresh: {}", e);
        Vec::new()
    });
    recent.retain(|item| item.case_root != case_root.display().to_string());
    recent.insert(
        0,
        RecentCaseDto {
            case_root: case_root.display().to_string(),
            name: summary.name.clone(),
            opened_at: chrono::Utc::now().to_rfc3339(),
        },
    );
    recent.truncate(MAX_RECENT_CASES);
    save_recent_cases(&recent)
}

fn read_recent_cases() -> Result<Vec<RecentCaseDto>, CommandError> {
    let path = recent_cases_path()?;
    if !path.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(&path).map_err(|e| {
        tracing::error!("Failed to read recent cases file: {}", e);
        CommandError::internal("Failed to read recent cases")
    })?;
    let parsed: Vec<RecentCaseDto> = serde_json::from_str(&content).map_err(|e| {
        tracing::error!("Failed to parse recent cases JSON: {}", e);
        CommandError::internal("Failed to read recent cases")
    })?;
    Ok(parsed
        .into_iter()
        .filter(|item| valid_recent_case_root(&item.case_root))
        .take(MAX_RECENT_CASES)
        .collect())
}

fn seed_analysis_demo(active: &app_services::active_case::ActiveCase) -> Result<(), String> {
    let evidence_root = active.case_root.join("evidence").join("analysis-demo");
    if evidence_root.exists() {
        std::fs::remove_dir_all(&evidence_root).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&evidence_root).map_err(|e| e.to_string())?;

    let fixture_root = repo_root().join("testdata").join("fixtures").join("tiny");
    copy_dir_all(&fixture_root.join("logical"), &evidence_root)?;
    let evtx_src = fixture_root.join("evtx").join("system.evtx");
    let evtx_dest = evidence_root
        .join("Windows")
        .join("System32")
        .join("winevt")
        .join("Logs")
        .join("System.evtx");
    if let Some(parent) = evtx_dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::copy(&evtx_src, &evtx_dest)
        .map_err(|e| format!("copy tiny System.evtx fixture: {e}"))?;

    write_demo_file(
        &evidence_root.join("Users").join("alice").join("report.pdf"),
        b"%PDF-1.7\n% demo forensic report\n",
    )?;
    write_demo_file(
        &evidence_root
            .join("Users")
            .join("alice")
            .join("Downloads")
            .join("tool.exe"),
        b"MZdemo executable header",
    )?;
    write_demo_file(
        &evidence_root
            .join("Users")
            .join("alice")
            .join("Archive")
            .join("case-notes.zip"),
        b"PK\x03\x04demo zip payload",
    )?;

    let ds_id = DataSourceId(format!("demo-ds-{}", Uuid::new_v4()));
    let data_source = DataSource {
        id: ds_id.clone(),
        name: "Analysis Demo Logical Evidence".to_string(),
        kind: DataSourceKind::LogicalDirectory,
        source_path: evidence_root.clone(),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut entries = Vec::new();
    collect_demo_entries(&evidence_root, &evidence_root, &ds_id, None, &mut entries)?;
    active
        .with_conn(|conn| {
            DataSourceRepo::new(conn).insert(&active.meta.id, &data_source)?;
            FileRepo::new(conn).insert_batch(&entries)?;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."))
}

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    if !src.is_dir() {
        return Err(format!("analysis demo fixture missing: {}", src.display()));
    }
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        let target = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn write_demo_file(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

fn collect_demo_entries(
    root: &std::path::Path,
    path: &std::path::Path,
    data_source_id: &DataSourceId,
    parent_id: Option<FileEntryId>,
    entries: &mut Vec<FileEntry>,
) -> Result<(), String> {
    let mut children = std::fs::read_dir(path)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    children.sort_by_key(|entry| entry.path());

    for child in children {
        let child_path = child.path();
        let metadata = child.metadata().map_err(|e| e.to_string())?;
        let relative = child_path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let id = FileEntryId(format!("demo-file-{}", Uuid::new_v4()));
        let entry_type = if metadata.is_dir() {
            EntryType::Directory
        } else {
            EntryType::File
        };
        entries.push(FileEntry {
            id: id.clone(),
            parent_id: parent_id.clone(),
            data_source_id: data_source_id.clone(),
            path: relative,
            name: child.file_name().to_string_lossy().to_string(),
            entry_type: entry_type.clone(),
            size: if metadata.is_file() {
                Some(metadata.len())
            } else {
                None
            },
            ext: child_path
                .extension()
                .map(|ext| ext.to_string_lossy().to_string()),
            deleted: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        });
        if entry_type == EntryType::Directory {
            collect_demo_entries(root, &child_path, data_source_id, Some(id), entries)?;
        }
    }
    Ok(())
}

fn valid_recent_case_root(case_root: &str) -> bool {
    if case_root.trim().is_empty() || case_root.contains('\0') {
        return false;
    }
    let root = PathBuf::from(case_root);
    root.is_dir() && root.join("case.json").is_file() && root.join("app.db").is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_services::{analysis_service, file_service};

    #[test]
    fn analysis_demo_seed_supports_real_analysis_flow() {
        let root = std::env::temp_dir().join(format!(
            "forensics-workbench-analysis-demo-test-{}",
            Uuid::new_v4()
        ));
        let active = case_service::create_case(&root, "Analysis Demo", Some("Codex Demo"))
            .expect("create demo case");
        seed_analysis_demo(&active).expect("seed demo case");

        active
            .with_conn(|conn| {
                let info =
                    analysis_service::extract_system_info_for_case(conn, |file_id, max_bytes| {
                        file_service::read_file_header_by_id(conn, file_id, max_bytes)
                    });
                assert!(
                    matches!(
                        info.status,
                        transport::dto::AnalysisParseStatusDto::Parsed
                            | transport::dto::AnalysisParseStatusDto::Partial
                    ),
                    "expected parsed or partial system info, got {:?}",
                    info.status
                );
                assert!(info.computer_name.is_some());
                assert!(info.os_version.is_some());
                assert!(info
                    .provenance
                    .iter()
                    .any(|item| item.parser == "registry.system"));
                assert!(info
                    .provenance
                    .iter()
                    .any(|item| item.parser == "evtx.boot_shutdown"));

                let files = analysis_service::collect_file_entries(conn).expect("collect files");
                let classifications =
                    analysis_service::classify_files_by_magic(&files, 5000, |file_id| {
                        file_service::read_file_header_by_id(
                            conn,
                            file_id,
                            analysis_service::MAGIC_HEADER_LIMIT,
                        )
                    });
                let detected = classifications
                    .iter()
                    .flat_map(|category| category.files.iter())
                    .map(|file| file.file_type.as_str())
                    .collect::<Vec<_>>();
                assert!(detected.contains(&"PDF"));
                assert!(detected.contains(&"PE"));
                assert!(detected.contains(&"ZIP"));
                Ok(())
            })
            .expect("verify demo analysis");

        std::fs::remove_dir_all(root).ok();
    }
}
