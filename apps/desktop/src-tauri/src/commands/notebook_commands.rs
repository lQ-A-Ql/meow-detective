use tauri::State;
use transport::{
    dto::{
        EvidenceCitationDto, GraphNodeTypeDto, InvestigationStepDto, NotebookEntryDto,
        NotebookEntryStatusDto, NotebookEntryTypeDto,
    },
    CommandError,
};

use super::command_support::{get_case_connection, require_active_case};
use crate::state::AppState;

// ── Notebook entry commands ──────────────────────────────────────────────

#[tauri::command]
pub async fn create_notebook_entry(
    state: State<'_, AppState>,
    case_id: String,
    author: String,
    entry_type: NotebookEntryTypeDto,
    title: String,
    body_markdown: String,
    tags: Vec<String>,
    status: NotebookEntryStatusDto,
    parent_id: Option<String>,
) -> Result<NotebookEntryDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::notebook_service::create_entry(
            &conn,
            &case_id,
            &author,
            &entry_type,
            &title,
            &body_markdown,
            &tags,
            &status,
            parent_id.as_deref(),
        )
        .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn update_notebook_entry(
    state: State<'_, AppState>,
    entry_id: String,
    title: Option<String>,
    body_markdown: Option<String>,
    tags: Option<Vec<String>>,
    status: Option<NotebookEntryStatusDto>,
) -> Result<NotebookEntryDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::notebook_service::update_entry(
            &conn,
            &entry_id,
            title.as_deref(),
            body_markdown.as_deref(),
            tags.as_deref(),
            status.as_ref(),
        )
        .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn list_notebook_entries(
    state: State<'_, AppState>,
    case_id: String,
    entry_type: Option<NotebookEntryTypeDto>,
    status: Option<NotebookEntryStatusDto>,
    tags: Option<Vec<String>>,
    search: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<NotebookEntryDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;

        let filters = persistence_sqlite::repositories::notebook_repo::NotebookEntryFilters {
            entry_type: entry_type.map(|et| match et {
                NotebookEntryTypeDto::Observation => domain::NotebookEntryType::Observation,
                NotebookEntryTypeDto::Hypothesis => domain::NotebookEntryType::Hypothesis,
                NotebookEntryTypeDto::Finding => domain::NotebookEntryType::Finding,
                NotebookEntryTypeDto::ActionItem => domain::NotebookEntryType::ActionItem,
                NotebookEntryTypeDto::Conclusion => domain::NotebookEntryType::Conclusion,
            }),
            status: status.map(|s| match s {
                NotebookEntryStatusDto::Draft => domain::EntryStatus::Draft,
                NotebookEntryStatusDto::Reviewed => domain::EntryStatus::Reviewed,
                NotebookEntryStatusDto::Final => domain::EntryStatus::Final,
            }),
            tags,
            search,
            limit,
            offset,
        };

        app_services::notebook_service::list_entries(&conn, &case_id, &filters)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn get_notebook_thread(
    state: State<'_, AppState>,
    entry_id: String,
) -> Result<Vec<NotebookEntryDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::notebook_service::get_thread(&conn, &entry_id)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

// ── Evidence citation commands ───────────────────────────────────────────

#[tauri::command]
pub async fn add_evidence_citation(
    state: State<'_, AppState>,
    entry_id: String,
    target_node_type: GraphNodeTypeDto,
    target_node_id: String,
    display_label: String,
    snippet: Option<String>,
) -> Result<EvidenceCitationDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::notebook_service::add_citation(
            &conn,
            &entry_id,
            &target_node_type,
            &target_node_id,
            &display_label,
            snippet.as_deref(),
        )
        .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

// ── Investigation step commands ──────────────────────────────────────────

#[tauri::command]
pub async fn list_investigation_steps(
    state: State<'_, AppState>,
    case_id: String,
    step_kind: Option<String>,
    success: Option<bool>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<InvestigationStepDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;

        let filters = persistence_sqlite::repositories::notebook_repo::StepFilters {
            step_kind,
            success,
            limit,
            offset,
        };

        app_services::notebook_service::list_steps(&conn, &case_id, &filters)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
