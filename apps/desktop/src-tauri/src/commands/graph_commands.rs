use tauri::State;
use transport::{
    dto::{
        GetNodeNeighborhoodRequest, GetProvenanceChainRequest, GraphNodeDto,
        GraphProvenanceEntryDto, GraphQueryDto, GraphQueryResultDto, GraphSnapshotDto,
        ListGraphNodesRequest,
    },
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
        app_services::graph_service::get_graph_snapshot_for_case(
            &conn,
            &snapshot.case_root,
            &snapshot.case_id,
        )
        .map_err(CommandError::from_typed_service_error)
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
        let snapshot = require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::graph_service::query_graph_for_case(
            &conn,
            &snapshot.case_root,
            &snapshot.case_id,
            query,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn list_graph_nodes(
    state: State<'_, AppState>,
    request: ListGraphNodesRequest,
) -> Result<Vec<GraphNodeDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::graph_service::list_graph_nodes_for_case(
            &conn,
            &snapshot.case_root,
            &snapshot.case_id,
            request,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn get_node_neighborhood(
    state: State<'_, AppState>,
    request: GetNodeNeighborhoodRequest,
) -> Result<GraphQueryResultDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::graph_service::get_node_neighborhood_for_case(
            &conn,
            &snapshot.case_root,
            &snapshot.case_id,
            &request.node_id,
            request.depth,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn get_provenance_chain(
    state: State<'_, AppState>,
    request: GetProvenanceChainRequest,
) -> Result<Vec<GraphProvenanceEntryDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = require_active_case(&app_state)?;
        let conn = get_case_connection(&app_state)?;
        app_services::graph_service::get_provenance_chain_for_case(
            &conn,
            &snapshot.case_root,
            &snapshot.case_id,
            &request.edge_id,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
