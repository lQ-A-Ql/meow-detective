use std::{
    collections::{HashMap, VecDeque},
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
    queue: VecDeque<(String, u64)>,
    owners: HashMap<String, u64>,
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
                    owners: HashMap::new(),
                    next_ticket: 0,
                }),
                changed: Condvar::new(),
            }),
        }
    }

    pub fn acquire(
        &self,
        group: &str,
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
        state.queue.push_back((group.to_string(), ticket));
        loop {
            if cancel_token.load(Ordering::Acquire) {
                state.queue.retain(|(_, candidate)| *candidate != ticket);
                self.inner.changed.notify_all();
                return Err(ImportGroupGateError::Cancelled);
            }
            let group_is_free = !state.owners.contains_key(group);
            let ticket_position = state
                .queue
                .iter()
                .position(|(candidate_group, _)| candidate_group == group);
            if group_is_free
                && ticket_position.is_some_and(|position| state.queue[position].1 == ticket)
            {
                state
                    .queue
                    .remove(ticket_position.expect("ticket position exists"));
                state.owners.insert(group.to_string(), ticket);
                return Ok(ImportGroupLease {
                    inner: Arc::clone(&self.inner),
                    group: group.to_string(),
                    ticket,
                });
            }
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                state.queue.retain(|(_, candidate)| *candidate != ticket);
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
    group: String,
    ticket: u64,
}

impl Drop for ImportGroupLease {
    fn drop(&mut self) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if state.owners.get(&self.group) == Some(&self.ticket) {
            state.owners.remove(&self.group);
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
