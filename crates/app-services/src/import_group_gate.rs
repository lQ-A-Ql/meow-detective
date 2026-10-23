use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, OnceLock,
    },
    time::Duration,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportGroupGateError {
    Cancelled,
    TimedOut,
}

struct GateState {
    queue: VecDeque<u64>,
    owner: Option<u64>,
    next_ticket: u64,
}

struct GateInner {
    state: Mutex<GateState>,
    changed: Condvar,
}

#[derive(Clone)]
pub struct ImportGroupGate {
    inner: Arc<GateInner>,
}

impl ImportGroupGate {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(GateInner {
                state: Mutex::new(GateState {
                    queue: VecDeque::new(),
                    owner: None,
                    next_ticket: 0,
                }),
                changed: Condvar::new(),
            }),
        }
    }

    pub fn acquire(
        &self,
        cancel_token: &AtomicBool,
        timeout: Duration,
    ) -> Result<ImportGroupLease, ImportGroupGateError> {
        let started = std::time::Instant::now();
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let ticket = state.next_ticket;
        state.next_ticket = state.next_ticket.wrapping_add(1);
        state.queue.push_back(ticket);
        loop {
            if cancel_token.load(Ordering::Acquire) {
                state.queue.retain(|candidate| *candidate != ticket);
                self.inner.changed.notify_all();
                return Err(ImportGroupGateError::Cancelled);
            }
            if state.owner.is_none() && state.queue.front() == Some(&ticket) {
                state.queue.pop_front();
                state.owner = Some(ticket);
                return Ok(ImportGroupLease {
                    inner: Arc::clone(&self.inner),
                    ticket,
                });
            }
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                state.queue.retain(|candidate| *candidate != ticket);
                self.inner.changed.notify_all();
                return Err(ImportGroupGateError::TimedOut);
            }
            state = self
                .inner
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(|error| error.into_inner())
                .0;
        }
    }
}

impl Default for ImportGroupGate {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ImportGroupLease {
    inner: Arc<GateInner>,
    ticket: u64,
}

impl Drop for ImportGroupLease {
    fn drop(&mut self) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if state.owner == Some(self.ticket) {
            state.owner = None;
            self.inner.changed.notify_all();
        }
    }
}

pub fn global_import_group_gate() -> &'static ImportGroupGate {
    static GATE: OnceLock<ImportGroupGate> = OnceLock::new();
    GATE.get_or_init(ImportGroupGate::new)
}

#[cfg(test)]
#[path = "../tests/unit/import_group_gate.rs"]
mod tests;
