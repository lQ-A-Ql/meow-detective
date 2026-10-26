use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;
use transport::CommandError;

#[derive(Default)]
pub(super) struct Admission {
    pending: Mutex<HashMap<String, usize>>,
}

struct Waiting {
    admission: Arc<Admission>,
    principal_id: String,
}

impl Drop for Waiting {
    fn drop(&mut self) {
        let mut pending = self
            .admission
            .pending
            .lock()
            .expect("MCP admission lock poisoned");
        let count = pending
            .get_mut(&self.principal_id)
            .expect("MCP pending entry must exist");
        *count -= 1;
        if *count == 0 {
            pending.remove(&self.principal_id);
        }
    }
}

impl Admission {
    pub(super) async fn acquire(
        self: &Arc<Self>,
        requests: Arc<Semaphore>,
        principal_id: &str,
        cancel: &CancellationToken,
        desktop: bool,
    ) -> Result<OwnedSemaphorePermit, CommandError> {
        if cancel.is_cancelled() {
            return Err(CommandError::cancelled("MCP 请求已取消"));
        }
        if let Ok(permit) = requests.clone().try_acquire_owned() {
            return Ok(permit);
        }
        let waiting = {
            let mut pending = self
                .pending
                .lock()
                .map_err(|_| CommandError::internal("MCP 队列不可用"))?;
            let count = pending.get(principal_id).copied().unwrap_or(0);
            if pending.values().sum::<usize>() >= 16 || count >= if desktop { 16 } else { 4 } {
                return Err(CommandError::conflict("MCP 请求队列已满"));
            }
            pending.insert(principal_id.into(), count + 1);
            Waiting {
                admission: self.clone(),
                principal_id: principal_id.into(),
            }
        };
        let permit = tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(CommandError::cancelled("MCP 请求已取消")),
            result = tokio::time::timeout(std::time::Duration::from_secs(5), requests.acquire_owned()) => {
                result.map_err(|_| CommandError::timeout("MCP 请求排队超时"))?
                    .map_err(|_| CommandError::internal("MCP 队列不可用"))
            }
        };
        drop(waiting);
        permit
    }
}

#[cfg(test)]
#[path = "../../tests/unit/mcp_host/admission.rs"]
mod tests;
