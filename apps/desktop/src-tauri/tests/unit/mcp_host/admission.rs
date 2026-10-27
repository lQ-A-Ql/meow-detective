use super::*;
use crate::{
    mcp_host::{execution::call_tool_as, invocation::Invocation},
    state::AppState,
};
use serde_json::json;
use transport::dto::mcp_host::McpHostToolCallRequestDto;

async fn wait_pending(admission: &Admission, expected: usize) {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            if admission.pending.lock().unwrap().values().sum::<usize>() == expected {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rejected_requests_return_typed_errors_and_audit_only_safe_metadata() {
    let temporary = tempfile::tempdir().unwrap();
    let state = AppState {
        mcp_host_settings_path: temporary.path().join("mcp-host.json"),
        ..Default::default()
    };
    *state.active_case.lock().unwrap() = Some(
        app_services::case_service::create_case(temporary.path(), "Request audit", None).unwrap(),
    );
    let created = crate::mcp_host::tests::grant(&state, false);
    let principal = state
        .mcp_host
        .access
        .lock()
        .unwrap()
        .authenticate(&created.token)
        .unwrap();
    let occupied = state
        .mcp_host
        .requests
        .clone()
        .acquire_many_owned(2)
        .await
        .unwrap();
    let request = McpHostToolCallRequestDto {
        name: "forensics.get_current_case".into(),
        arguments: json!({}),
    };
    let mut waiting = Vec::new();
    for _ in 0..4 {
        let (state, request, principal) = (state.clone(), request.clone(), principal.clone());
        waiting.push(tokio::spawn(async move {
            call_tool_as(state, request, Some(principal), CancellationToken::new())
                .await
                .unwrap()
        }));
    }
    wait_pending(&state.mcp_host.admission, 4).await;
    let full = call_tool_as(
        state.clone(),
        request.clone(),
        Some(principal.clone()),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    let error = full.error_details.unwrap();
    assert_eq!(error.code, "MCP_QUEUE_FULL");
    assert!(error.recoverable);
    for waiter in waiting {
        let response = waiter.await.unwrap();
        let error = response.error_details.unwrap();
        assert_eq!(error.code, "MCP_QUEUE_TIMEOUT");
        assert_eq!(error.category.as_deref(), Some("timeout"));
    }
    let unknown = call_tool_as(
        state.clone(),
        McpHostToolCallRequestDto {
            name: "private-input-must-not-enter-audit".into(),
            arguments: json!({"secret":"hidden"}),
        },
        Some(principal.clone()),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(unknown.error_details.unwrap().code, "MCP_UNKNOWN_TOOL");
    let cancel = CancellationToken::new();
    cancel.cancel();
    let cancelled = call_tool_as(state.clone(), request, Some(principal), cancel)
        .await
        .unwrap();
    assert_eq!(cancelled.error_details.unwrap().code, "CANCELLED");
    let connection = state.get_connection().unwrap();
    let events = persistence_sqlite::repositories::audit_repo::AuditRepo::new(&connection)
        .query(None, Some("mcp.tool.call"), 20, 0)
        .unwrap();
    assert_eq!(events.len(), 7);
    for event in events {
        assert_eq!(event.user_id, created.client.id);
        assert!(!event.details.contains("private-input"));
        assert!(!event.details.contains("hidden"));
        let details: serde_json::Value = serde_json::from_str(&event.details).unwrap();
        assert_eq!(details["success"], false);
        assert!(details["requestId"].is_string());
        assert!(details["errorCode"].is_string());
    }
    let audit_waiters = state
        .mcp_host
        .audit_waiters
        .clone()
        .acquire_many_owned(16)
        .await
        .unwrap();
    let overloaded = call_tool_as(
        state.clone(),
        McpHostToolCallRequestDto {
            name: "unknown".into(),
            arguments: json!({}),
        },
        None,
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(overloaded.error_details.unwrap().code, "MCP_AUDIT_BUSY");
    drop(audit_waiters);
    drop(occupied);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn waiting_is_bounded_and_cancelled_waiters_release_capacity() {
    let admission = Arc::new(Admission::default());
    let permits = Arc::new(Semaphore::new(2));
    let occupied = permits.clone().acquire_many_owned(2).await.unwrap();
    let cancel = CancellationToken::new();
    let mut tasks = Vec::new();
    for index in 0..4 {
        let (admission, permits, cancel) = (admission.clone(), permits.clone(), cancel.clone());
        tasks.push(tokio::spawn(async move {
            admission
                .acquire(permits, &format!("client-{}", index / 4), &cancel, false)
                .await
        }));
    }
    wait_pending(&admission, 4).await;
    assert_eq!(
        admission
            .acquire(permits.clone(), "client-0", &cancel, false)
            .await
            .unwrap_err()
            .code,
        "CONFLICT"
    );
    for index in 4..16 {
        let (admission, permits, cancel) = (admission.clone(), permits.clone(), cancel.clone());
        tasks.push(tokio::spawn(async move {
            admission
                .acquire(permits, &format!("client-{}", index / 4), &cancel, false)
                .await
        }));
    }
    wait_pending(&admission, 16).await;
    assert_eq!(
        admission
            .acquire(permits.clone(), "overflow", &cancel, false)
            .await
            .unwrap_err()
            .code,
        "CONFLICT"
    );
    cancel.cancel();
    for task in tasks {
        assert_eq!(task.await.unwrap().unwrap_err().code, "CANCELLED");
    }
    assert!(admission.pending.lock().unwrap().is_empty());
    drop(occupied);
    assert_eq!(permits.available_permits(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queued_calls_cannot_follow_a_case_switch_or_a_revoked_grant() {
    let temporary = tempfile::tempdir().unwrap();
    let state = AppState {
        mcp_host_settings_path: temporary.path().join("mcp-host.json"),
        ..Default::default()
    };
    *state.active_case.lock().unwrap() = Some(
        app_services::case_service::create_case(temporary.path(), "Queued case", None).unwrap(),
    );
    let occupied = state
        .mcp_host
        .requests
        .clone()
        .acquire_many_owned(2)
        .await
        .unwrap();
    let queued_state = state.clone();
    let task = tokio::spawn(async move {
        call_tool_as(
            queued_state,
            McpHostToolCallRequestDto {
                name: "forensics.get_current_case".into(),
                arguments: json!({}),
            },
            None,
            CancellationToken::new(),
        )
        .await
        .unwrap()
    });
    wait_pending(&state.mcp_host.admission, 1).await;
    *state.active_case.lock().unwrap() = Some(
        app_services::case_service::create_case(temporary.path(), "Other case", None).unwrap(),
    );
    drop(occupied);
    let result = task.await.unwrap();
    assert!(!result.success);
    assert!(result.error.unwrap().contains("失效"));
    let created = crate::mcp_host::tests::grant(&state, false);
    let principal = state
        .mcp_host
        .access
        .lock()
        .unwrap()
        .authenticate(&created.token)
        .unwrap();
    let occupied = state
        .mcp_host
        .requests
        .clone()
        .acquire_many_owned(2)
        .await
        .unwrap();
    let queued_state = state.clone();
    let task = tokio::spawn(async move {
        call_tool_as(
            queued_state,
            McpHostToolCallRequestDto {
                name: "forensics.get_current_case".into(),
                arguments: json!({}),
            },
            Some(principal),
            CancellationToken::new(),
        )
        .await
        .unwrap()
    });
    wait_pending(&state.mcp_host.admission, 1).await;
    state
        .mcp_host
        .revoke_client(&state, &created.client.id)
        .unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
    assert!(!result.success);
    assert!(state.mcp_host.admission.pending.lock().unwrap().is_empty());
    drop(occupied);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancellation_interrupts_sql_and_retains_execution_until_completion() {
    let temporary = tempfile::tempdir().unwrap();
    let state = AppState::default();
    *state.active_case.lock().unwrap() = Some(
        app_services::case_service::create_case(temporary.path(), "SQL cancellation", None)
            .unwrap(),
    );
    let cancel = CancellationToken::new();
    let invocation = Invocation::new(&state, None, cancel.clone(), false).unwrap();
    let (ready, started) = tokio::sync::oneshot::channel();
    let task = tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let _guard = invocation.interrupt_sql(&conn);
        ready.send(()).unwrap();
        conn.query_row("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000000) SELECT sum(x) FROM n", [], |row| row.get::<_, i64>(0))
    });
    started.await.unwrap();
    cancel.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(result, Err(rusqlite::Error::SqliteFailure(error, _)) if error.code == rusqlite::ErrorCode::OperationInterrupted)
    );
}
