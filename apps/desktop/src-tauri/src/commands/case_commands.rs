use app_services::{case_service, job_service};
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

use crate::{events::event_bridge, state::AppState};

use super::settings_commands::load_app_settings;

fn init_case_db(state: &AppState) -> Result<(), CommandError> {
    state
        .init_db_pragmas()
        .map_err(CommandError::from_service_error)
}

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

fn same_path(left: &std::path::Path, right: &std::path::Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
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
    let db_path = active.db_path();
    let active_case_root = active.case_root.clone();
    init_case_db(&state)?;

    let dto = meta_to_dto(&active.meta);
    let mut guard = state
        .active_case
        .lock()
        .map_err(|e| CommandError::from_lock_error("Case", e))?;
    *guard = Some(active);
    remember_recent_case(&active_case_root, &dto)?;
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
    let db_path = active.db_path();
    init_case_db(&state)?;

    // Recover any jobs that were left in a running/cancelling state from a
    // previous app crash or unexpected shutdown.  This is best-effort;
    // case opening continues even if recovery fails.
    match state.get_connection() {
        Ok(conn) => match job_service::recover_interrupted_jobs(&conn) {
            Ok(recovery) => {
                if !recovery.recovered_job_ids.is_empty() {
                    tracing::info!(
                        "Recovered {} interrupted job(s): {:?}",
                        recovery.recovered_job_ids.len(),
                        recovery.recovered_job_ids
                    );
                }
            }
            Err(error) => {
                tracing::warn!("Failed to recover interrupted jobs on case open: {error}");
            }
        },
        Err(error) => {
            tracing::warn!("Failed to get connection for job recovery on case open: {error}");
        }
    }

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
    app_services::analysis_service::seed_analysis_demo_data(&active)
        .map_err(CommandError::from_service_error)?;
    let db_path = active.db_path();
    init_case_db(&state)?;

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
    let timeout = std::time::Duration::from_secs(5);
    let _ = state.task_manager.wait_all(timeout);

    // 3. Drain database jobs �?mark any that are still running as interrupted
    match state.get_connection() {
        Ok(conn) => {
            let case_id = {
                let guard = state
                    .active_case
                    .lock()
                    .map_err(|e| CommandError::from_lock_error("Case", e))?;
                guard
                    .as_ref()
                    .map(|active| active.meta.id.0.clone())
                    .unwrap_or_default()
            };
            match case_service::close_case_drain(&conn, &case_id, timeout.as_millis() as u64) {
                Ok(drain) => {
                    if !drain.fully_drained {
                        tracing::warn!(
                            "Degraded case close �?{} job(s) did not drain within {}ms: {:?}",
                            drain.pending_jobs.len(),
                            timeout.as_millis(),
                            drain.pending_jobs,
                        );
                        for warning in &drain.warnings {
                            tracing::warn!("{}", warning);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to drain jobs during case close: {}", e);
                }
            }
        }
        Err(e) => {
            tracing::warn!("Failed to get connection for case close drain: {}", e);
        }
    }

    // 4. Clear pooled database handles before clearing the active case.
    state
        .clear_db_state()
        .map_err(CommandError::from_service_error)?;

    // 5. Clear active case
    let mut guard = state
        .active_case
        .lock()
        .map_err(|e| CommandError::from_lock_error("Case", e))?;
    let closed_case_id = guard.as_ref().map(|active| active.meta.id.0.clone());
    *guard = None;
    drop(guard);
    if let Some(case_id) = &closed_case_id {
        let _ = state.clear_runtime_cache_for_case(case_id);
    }
    app_services::file_service::clear_e01_reader_cache();
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
        // Guard is now dropped �?query with a fresh connection
        let conn = app_services::connection::open_case_db(&db_path)
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
        let conn = app_services::connection::open_case_db(&db_path)
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
        let conn = app_services::connection::open_case_db(&db_path)
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
        // Guard is now dropped �?query with released lock
        let conn = app_services::connection::open_case_db(&db_path)
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

    // Use user-configured cases directory for validation instead of hardcoded default
    let settings = load_app_settings(&state.app_settings_path)?;
    let allowed_root = PathBuf::from(&settings.case_root);

    let mut cleared_active_case = false;
    let mut cleared_case_id: Option<String> = None;
    {
        let mut guard = state
            .active_case
            .lock()
            .map_err(|e| CommandError::from_lock_error("Case", e))?;
        if let Some(ref active) = *guard {
            if same_path(&active.case_root, &root) {
                cleared_case_id = Some(active.meta.id.0.clone());
                *guard = None;
                cleared_active_case = true;
            }
        }
    }
    if cleared_active_case {
        state
            .clear_db_state()
            .map_err(CommandError::from_service_error)?;
        if let Some(case_id) = cleared_case_id.as_deref() {
            let _ = state.clear_runtime_cache_for_case(case_id);
        }
    }

    let root_clone = root.clone();
    let allowed_root_clone = allowed_root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        case_service::delete_case_in(&root_clone, &allowed_root_clone)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)??;

    let mut recent = read_recent_cases().unwrap_or_else(|e| {
        tracing::warn!("Failed to read recent cases, starting fresh: {}", e);
        Vec::new()
    });
    recent.retain(|item| {
        item.case_root != request.case_root
            && !same_path(std::path::Path::new(&item.case_root), &root)
    });
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
        // Guard is now dropped �?query with released lock
        let conn = app_services::connection::open_case_db(&db_path)
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
    // FORENSICS_RECENT_CASES_DIR is intended for tests; in production it is not set.
    let base = std::env::var_os("FORENSICS_RECENT_CASES_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
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

    if let Err(e) = crate::platform_security::restrict_file_to_current_user(&path) {
        tracing::error!("Failed to restrict recent cases file ACL: {}", e);
        return Err(CommandError::security("Failed to secure recent cases file"));
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
    use crate::commands::command_support::{get_case_connection, require_active_case};
    use app_services::{analysis_service, file_service};
    use uuid::Uuid;

    #[test]
    fn active_case_pool_is_guarded_by_active_case_lifecycle() {
        let root = std::env::temp_dir().join(format!(
            "forensics-workbench-pool-lifecycle-test-{}",
            Uuid::new_v4()
        ));
        let active = case_service::create_case(&root, "Pool Lifecycle", Some("Codex Test"))
            .expect("create test case");
        let db_path = active.db_path();
        let state = AppState::default();

        init_case_db(&state).expect("initialize pool");
        let no_active_case = state
            .get_connection()
            .expect_err("pool access must require active case");
        assert!(no_active_case.contains("No active case"));

        *state.active_case.lock().expect("active case lock") = Some(active);
        state
            .get_connection()
            .expect("pool available when active case is set");

        state.clear_db_state().expect("clear pool");
        *state.active_case.lock().expect("active case lock") = None;
        let cleared = state
            .get_connection()
            .expect_err("cleared pool must not be usable");
        assert!(cleared.contains("No active case"));

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn command_support_helpers_follow_active_case_lifecycle() {
        let root = std::env::temp_dir().join(format!(
            "forensics-workbench-command-helper-lifecycle-test-{}",
            Uuid::new_v4()
        ));
        let active = case_service::create_case(&root, "Lifecycle", Some("Codex Test"))
            .expect("create test case");
        let db_path = active.db_path();
        let state = AppState::default();

        init_case_db(&state).expect("initialize pool");
        let no_case = require_active_case(&state).expect_err("active case required");
        assert_eq!(no_case.code, "NO_ACTIVE_CASE");

        *state.active_case.lock().expect("active case lock") = Some(active);
        let snapshot = require_active_case(&state).expect("snapshot available");
        assert_eq!(snapshot.db_path, db_path);
        get_case_connection(&state).expect("pooled connection available");

        *state.active_case.lock().expect("active case lock") = None;
        let no_case_again = get_case_connection(&state).expect_err("connection requires case");
        assert_eq!(no_case_again.code, "NO_ACTIVE_CASE");

        state.clear_db_state().expect("clear pool");
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn analysis_demo_seed_supports_real_analysis_flow() {
        let root = std::env::temp_dir().join(format!(
            "forensics-workbench-analysis-demo-test-{}",
            Uuid::new_v4()
        ));
        let active = case_service::create_case(&root, "Analysis Demo", Some("Codex Demo"))
            .expect("create demo case");
        app_services::analysis_service::seed_analysis_demo_data(&active).expect("seed demo case");

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

    #[test]
    fn remember_recent_case_uses_actual_case_directory() {
        let parent = std::env::temp_dir().join(format!(
            "forensics-workbench-recent-case-test-{}",
            Uuid::new_v4()
        ));
        let active = case_service::create_case(&parent, "recent-case", Some("tester"))
            .expect("create recent case fixture");
        let dto = meta_to_dto(&active.meta);
        let actual_case_root = active.case_root.clone();
        drop(active);

        remember_recent_case(&actual_case_root, &dto).expect("remember recent case");
        let recent = read_recent_cases().expect("read recent cases");

        assert_eq!(recent[0].case_root, actual_case_root.display().to_string());
        assert_ne!(recent[0].case_root, parent.display().to_string());

        let mut remaining = recent;
        remaining.retain(|item| item.case_root != actual_case_root.display().to_string());
        save_recent_cases(&remaining).expect("restore recent cases");
        std::fs::remove_dir_all(parent).ok();
    }

    #[test]
    fn recent_cases_file_is_restricted_and_round_trips() {
        use std::sync::Mutex;

        static LOCK: Mutex<()> = Mutex::new(());
        let _lock = LOCK.lock().unwrap();

        let dir = std::env::temp_dir().join(format!(
            "forensics-recent-cases-security-test-{}",
            Uuid::new_v4()
        ));
        let previous = std::env::var_os("FORENSICS_RECENT_CASES_DIR");
        std::env::set_var("FORENSICS_RECENT_CASES_DIR", &dir);

        let active =
            case_service::create_case(&dir, "Secure Case", Some("tester")).expect("create case");
        let summary = meta_to_dto(&active.meta);
        let cases = vec![RecentCaseDto {
            case_root: active.case_root.display().to_string(),
            name: summary.name,
            opened_at: chrono::Utc::now().to_rfc3339(),
        }];

        let saved = save_recent_cases(&cases);
        assert!(saved.is_ok(), "save_recent_cases failed: {saved:?}");

        let path = recent_cases_path().expect("resolve recent cases path");
        assert!(path.exists(), "recent cases file should exist");

        let loaded = read_recent_cases().expect("read recent cases");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "Secure Case");

        match previous {
            Some(v) => std::env::set_var("FORENSICS_RECENT_CASES_DIR", v),
            None => std::env::remove_var("FORENSICS_RECENT_CASES_DIR"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
