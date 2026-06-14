use tauri::State;
use transport::{
    dto::{GraphProvenanceEntryDto, GraphQueryDto, GraphQueryResultDto, GraphSnapshotDto},
    CommandError,
};

use super::command_support::{get_case_connection, require_active_case};
use crate::state::AppState;

#[tauri::command]
pub async fn get_graph_snapshot(
    state: State<'_, AppState>,
) -> Result<GraphSnapshotDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::graph_service::get_graph_snapshot(&conn, &snapshot.case_id)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn query_graph(
    state: State<'_, AppState>,
    query: GraphQueryDto,
) -> Result<GraphQueryResultDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::graph_service::query_graph(&conn, query)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn get_node_neighborhood(
    state: State<'_, AppState>,
    node_id: String,
    depth: u32,
) -> Result<GraphQueryResultDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::graph_service::get_node_neighborhood(&conn, &node_id, depth)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn get_provenance_chain(
    state: State<'_, AppState>,
    edge_id: String,
) -> Result<Vec<GraphProvenanceEntryDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::graph_service::get_provenance_chain(&conn, &edge_id)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
