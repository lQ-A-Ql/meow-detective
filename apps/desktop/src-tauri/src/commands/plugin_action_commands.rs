//! Plugin action channel commands (ABI doc §3 optional export): the generic
//! action listing and the WeChat database-key recovery entry point. Thin
//! wrappers over `app-services`; no business logic here.

use app_services::{plugin_action_service, wechat_key_service};
use domain::DataSourceId;
use std::path::PathBuf;
use tauri::State;
use transport::{
    commands::{ListPluginActionsRequest, RecoverWeChatKeysRequest},
    dto::{PluginActionDescriptorDto, WeChatKeyRecoveryResultDto},
    CommandError,
};

use crate::commands::command_support::{get_case_connection, require_active_case};
use crate::state::AppState;

#[tauri::command]
pub async fn list_plugin_actions(
    request: ListPluginActionsRequest,
) -> Result<Vec<PluginActionDescriptorDto>, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    tauri::async_runtime::spawn_blocking(move || {
        plugin_action_service::list_plugin_actions(&request.plugin_id)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn recover_wechat_keys(
    state: State<'_, AppState>,
    request: RecoverWeChatKeysRequest,
) -> Result<WeChatKeyRecoveryResultDto, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let data_source_id = DataSourceId(request.data_source_id);
    let dump_path = PathBuf::from(request.dump_path);

    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let case_conn = get_case_connection(&app_state)?;
        wechat_key_service::recover_wechat_keys(
            &case_conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
            &dump_path,
            crate::platform_security::restrict_file_to_current_user,
        )
        .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
