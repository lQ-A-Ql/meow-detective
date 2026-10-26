use crate::state::AppState;
use app_services::mcp_host_service::access::McpPrincipal;
use tokio_util::sync::CancellationToken;
use transport::CommandError;

pub(super) struct Invocation {
    pub case_id: String,
    pub session_id: String,
    pub actor: String,
    pub principal: Option<McpPrincipal>,
    pub cancel: CancellationToken,
    monitor: tokio::task::JoinHandle<()>,
}

pub(super) struct SqlInterruption(tokio::task::JoinHandle<()>);

impl Drop for SqlInterruption {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl Invocation {
    pub(super) fn new(
        state: &AppState,
        principal: Option<McpPrincipal>,
        request_cancel: CancellationToken,
        content: bool,
    ) -> Result<Self, CommandError> {
        let (case_id, session_id) = {
            let active = state
                .active_case
                .lock()
                .map_err(|_| CommandError::internal("案件状态不可用"))?;
            let active = active.as_ref().ok_or_else(CommandError::no_active_case)?;
            (active.meta.id.0.clone(), active.session_id.clone())
        };
        if principal.as_ref().is_some_and(|p| {
            !p.active() || p.client.case_id != case_id || p.client.case_session_id != session_id
        }) {
            return Err(CommandError::security("客户端授权与当前案件不匹配"));
        }
        let actor = principal
            .as_ref()
            .map(|p| p.client.id.clone())
            .unwrap_or_else(|| "desktop".into());
        let shutdown = state
            .mcp_host
            .execution_cancel
            .lock()
            .map_err(|_| CommandError::internal("MCP 状态不可用"))?
            .clone();
        let cancel = request_cancel.child_token();
        let monitor_cancel = cancel.clone();
        let grant = principal.clone();
        let monitor = tokio::spawn(async move {
            let deadline = tokio::time::sleep(std::time::Duration::from_secs(if content {
                60
            } else {
                30
            }));
            tokio::pin!(deadline);
            loop {
                tokio::select! {
                    _ = monitor_cancel.cancelled() => break,
                    _ = shutdown.cancelled() => { monitor_cancel.cancel(); break; },
                    _ = &mut deadline => { monitor_cancel.cancel(); break; },
                    _ = tokio::time::sleep(std::time::Duration::from_millis(20)) => {
                        if grant.as_ref().is_some_and(|p| !p.active()) { monitor_cancel.cancel(); break; }
                    }
                }
            }
        });
        Ok(Self {
            case_id,
            session_id,
            actor,
            principal,
            cancel,
            monitor,
        })
    }

    pub fn check(&self, name: &str) -> Result<(), CommandError> {
        if self.cancel.is_cancelled() {
            return Err(CommandError::cancelled("MCP 请求已取消或超过执行时间"));
        }
        if self.principal.as_ref().is_some_and(|p| {
            !app_services::mcp_host_service::tool_catalog()
                .iter()
                .find(|tool| tool.name == name)
                .is_some_and(|tool| p.allows(tool))
        }) {
            return Err(CommandError::security("客户端没有此项工具的访问权限"));
        }
        Ok(())
    }

    pub fn interrupt_sql(&self, connection: &rusqlite::Connection) -> SqlInterruption {
        let progress_cancel = self.cancel.clone();
        connection.progress_handler(1000, Some(move || progress_cancel.is_cancelled()));
        let interrupt = connection.get_interrupt_handle();
        let cancel = self.cancel.clone();
        SqlInterruption(tokio::spawn(async move {
            cancel.cancelled().await;
            interrupt.interrupt();
        }))
    }
}

impl Drop for Invocation {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.monitor.abort();
    }
}
