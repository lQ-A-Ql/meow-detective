use tauri::State;
use transport::{
    dto::{
        AddEvidenceCitationRequest, CreateNotebookEntryRequest, EvidenceCitationDto,
        GetNotebookThreadRequest, InvestigationStepDto, ListInvestigationStepsRequest,
        ListNotebookEntriesRequest, NotebookEntryDto, NotebookEntryStatusDto, NotebookEntryTypeDto,
        UpdateNotebookEntryRequest,
    },
    CommandError,
};

use super::command_support::{get_case_connection, require_active_case};
use crate::state::AppState;

// ── Notebook entry commands ──────────────────────────────────────────────

#[tauri::command]
pub async fn create_notebook_entry(
    state: State<'_, AppState>,
    request: CreateNotebookEntryRequest,
) -> Result<NotebookEntryDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let case_id = active.case_id;
        let conn = get_case_connection(&app_state)?;
        app_services::notebook_service::create_entry(
            &conn,
            &case_id,
            &request.author,
            &request.entry_type,
            &request.title,
            &request.body_markdown,
            &request.tags,
            &request.status,
            request.parent_id.as_deref(),
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn update_notebook_entry(
    state: State<'_, AppState>,
    request: UpdateNotebookEntryRequest,
) -> Result<NotebookEntryDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::notebook_service::update_entry(
            &conn,
            &request.entry_id,
            request.title.as_deref(),
            request.body_markdown.as_deref(),
            request.tags.as_deref(),
            request.status.as_ref(),
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn list_notebook_entries(
    state: State<'_, AppState>,
    request: ListNotebookEntriesRequest,
) -> Result<Vec<NotebookEntryDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let case_id = active.case_id;
        let conn = get_case_connection(&app_state)?;

        let filters = persistence_sqlite::repositories::notebook_repo::NotebookEntryFilters {
            entry_type: request.entry_type.map(|et| match et {
                NotebookEntryTypeDto::Observation => domain::NotebookEntryType::Observation,
                NotebookEntryTypeDto::Hypothesis => domain::NotebookEntryType::Hypothesis,
                NotebookEntryTypeDto::Finding => domain::NotebookEntryType::Finding,
                NotebookEntryTypeDto::ActionItem => domain::NotebookEntryType::ActionItem,
                NotebookEntryTypeDto::Conclusion => domain::NotebookEntryType::Conclusion,
            }),
            status: request.status.map(|s| match s {
                NotebookEntryStatusDto::Draft => domain::EntryStatus::Draft,
                NotebookEntryStatusDto::Reviewed => domain::EntryStatus::Reviewed,
                NotebookEntryStatusDto::Final => domain::EntryStatus::Final,
            }),
            tags: Some(request.tags),
            search: request.search,
            limit: request.limit,
            offset: request.offset,
        };

        app_services::notebook_service::list_entries(&conn, &case_id, &filters)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn get_notebook_thread(
    state: State<'_, AppState>,
    request: GetNotebookThreadRequest,
) -> Result<Vec<NotebookEntryDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::notebook_service::get_thread(&conn, &request.entry_id)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

// ── Evidence citation commands ───────────────────────────────────────────

#[tauri::command]
pub async fn add_evidence_citation(
    state: State<'_, AppState>,
    request: AddEvidenceCitationRequest,
) -> Result<EvidenceCitationDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::notebook_service::add_citation(
            &conn,
            &request.entry_id,
            &request.target_node_type,
            &request.target_node_id,
            &request.display_label,
            request.snippet.as_deref(),
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

// ── Investigation step commands ──────────────────────────────────────────

#[tauri::command]
pub async fn list_investigation_steps(
    state: State<'_, AppState>,
    request: ListInvestigationStepsRequest,
) -> Result<Vec<InvestigationStepDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let case_id = active.case_id;
        let conn = get_case_connection(&app_state)?;

        let filters = persistence_sqlite::repositories::notebook_repo::StepFilters {
            step_kind: request.step_kind,
            success: request.success,
            limit: request.limit,
            offset: request.offset,
        };

        app_services::notebook_service::list_steps(&conn, &case_id, &filters)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
