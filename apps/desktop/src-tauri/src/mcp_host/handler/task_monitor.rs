use crate::state::AppState;
use app_services::mcp_host_service::access::McpPrincipal;
use rmcp::{
    task_manager::{TaskContext, TaskExit},
    ErrorData,
};
use std::time::Duration;
use transport::dto::mcp_host::McpHostToolCallResultDto;

pub(super) async fn wait_for_background_task(
    state: &AppState,
    principal: &McpPrincipal,
    mut result: McpHostToolCallResultDto,
    task_context: &TaskContext,
) -> Result<McpHostToolCallResultDto, TaskExit> {
    let Some((task_key, task_id)) = result
        .data
        .as_ref()
        .and_then(serde_json::Value::as_object)
        .and_then(|data| {
            ["jobId", "taskId", "id"].into_iter().find_map(|key| {
                data.get(key)
                    .and_then(serde_json::Value::as_str)
                    .map(|id| (key, id.to_owned()))
            })
        })
    else {
        return Ok(result);
    };
    loop {
        if task_context.is_cancel_requested() {
            cancel_background_task(state, principal, &task_id);
            return Err(TaskExit::Cancelled);
        }
        let (status, snapshot) = read_background_task(state, principal, task_key, &task_id)?;
        if matches!(
            status.as_str(),
            "completed" | "failed" | "cancelled" | "warning"
        ) {
            let data = result
                .data
                .as_mut()
                .and_then(serde_json::Value::as_object_mut)
                .ok_or_else(|| {
                    TaskExit::Error(ErrorData::internal_error("任务结果不可用", None))
                })?;
            data.insert("status".into(), serde_json::Value::String(status));
            data.insert("task".into(), snapshot);
            return Ok(result);
        }
        tokio::select! {
            _ = task_context.cancelled() => {
                cancel_background_task(state, principal, &task_id);
                return Err(TaskExit::Cancelled);
            }
            _ = tokio::time::sleep(Duration::from_millis(250)) => {}
        }
    }
}

fn read_background_task(
    state: &AppState,
    principal: &McpPrincipal,
    task_key: &str,
    task_id: &str,
) -> Result<(String, serde_json::Value), TaskExit> {
    let active_guard = state
        .active_case
        .lock()
        .map_err(|_| TaskExit::Error(ErrorData::internal_error("案件状态不可用", None)))?;
    let active = active_guard
        .as_ref()
        .ok_or_else(|| TaskExit::Error(ErrorData::invalid_request("案件上下文已失效", None)))?;
    if active.meta.id.0 != principal.client.case_id
        || active.session_id != principal.client.case_session_id
    {
        return Err(TaskExit::Error(ErrorData::invalid_request(
            "案件上下文已失效",
            None,
        )));
    }
    let connection = app_services::connection::open_existing_case_db(&active.db_path())
        .map_err(|_| TaskExit::Error(ErrorData::internal_error("任务状态不可用", None)))?;
    if task_key == "jobId" || task_key == "id" {
        let snapshot =
            app_services::job_service::get_job_from_db(&connection, &domain::JobId(task_id.into()))
                .map_err(|_| TaskExit::Error(ErrorData::invalid_request("任务不可用", None)))?;
        Ok((
            snapshot.status.clone(),
            serde_json::to_value(snapshot)
                .map_err(|_| TaskExit::Error(ErrorData::internal_error("任务结果不可用", None)))?,
        ))
    } else {
        let snapshot = app_services::evidence_tasks::get(&connection, &active.meta.id, task_id)
            .map_err(|_| TaskExit::Error(ErrorData::invalid_request("任务不可用", None)))?;
        Ok((
            snapshot.state.clone(),
            serde_json::to_value(snapshot)
                .map_err(|_| TaskExit::Error(ErrorData::internal_error("任务结果不可用", None)))?,
        ))
    }
}

fn cancel_background_task(state: &AppState, principal: &McpPrincipal, task_id: &str) {
    let Ok(active) = state.active_case.lock() else {
        return;
    };
    let Some(active) = active.as_ref() else {
        return;
    };
    if active.meta.id.0 != principal.client.case_id
        || active.session_id != principal.client.case_session_id
    {
        return;
    }
    if let Ok(connection) = app_services::connection::open_existing_case_db(&active.db_path()) {
        let _ = app_services::job_service::request_cancellation(
            &connection,
            &domain::JobId(task_id.to_owned()),
            "Cancellation requested by MCP Tasks client",
        );
    }
    let _ = state.task_manager.cancel(task_id);
}
