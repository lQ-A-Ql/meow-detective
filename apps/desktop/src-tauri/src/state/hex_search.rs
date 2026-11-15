//! In-memory admission and lifecycle state for whole-file hex searches.

use std::collections::HashMap;
use std::sync::Mutex;

use transport::dto::{HexSearchModeDto, HexSearchPhaseDto, HexSearchStatusDto};

pub const MAX_HEX_SEARCH_JOBS: usize = 16;
pub const MAX_ACTIVE_HEX_SEARCHES: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HexSearchAdmission {
    Start(HexSearchStatusDto),
    Replace {
        cancelled_task_id: String,
        status: HexSearchStatusDto,
    },
    Reuse(HexSearchStatusDto),
    Busy,
}

struct Entry {
    status: HexSearchStatusDto,
    mode: HexSearchModeDto,
    pattern: Vec<u8>,
    seq: u64,
    updated: u64,
}

#[derive(Default)]
struct RegistryState {
    entries: HashMap<(String, String), Entry>,
    next_seq: u64,
    clock: u64,
}

#[derive(Default)]
pub struct HexSearchJobRegistry {
    state: Mutex<RegistryState>,
}

impl HexSearchJobRegistry {
    fn lock(&self) -> std::sync::MutexGuard<'_, RegistryState> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// Decide what a new request for `(case_id, file_id)` should do.
    ///
    /// A different query against a running search replaces it; the caller is
    /// responsible for cancelling `cancelled_task_id` through the task manager.
    pub fn admit(
        &self,
        case_id: &str,
        file_id: &str,
        mode: HexSearchModeDto,
        pattern: &[u8],
    ) -> HexSearchAdmission {
        let mut guard = self.lock();
        let state = &mut *guard;
        let key = (case_id.to_string(), file_id.to_string());
        if let Some(entry) = state.entries.get(&key) {
            if !entry.status.phase.is_terminal() {
                if entry.mode == mode && entry.pattern == pattern {
                    return HexSearchAdmission::Reuse(entry.status.clone());
                }
                let cancelled_task_id = entry.status.task_id.clone();
                let Some(status) = insert_entry(state, key, mode, pattern, file_id) else {
                    return HexSearchAdmission::Busy;
                };
                return HexSearchAdmission::Replace {
                    cancelled_task_id,
                    status,
                };
            }
            let Some(status) = insert_entry(state, key, mode, pattern, file_id) else {
                return HexSearchAdmission::Busy;
            };
            return HexSearchAdmission::Start(status);
        }
        let active = state
            .entries
            .values()
            .filter(|entry| !entry.status.phase.is_terminal())
            .count();
        if active >= MAX_ACTIVE_HEX_SEARCHES {
            return HexSearchAdmission::Busy;
        }
        let Some(status) = insert_entry(state, key, mode, pattern, file_id) else {
            return HexSearchAdmission::Busy;
        };
        HexSearchAdmission::Start(status)
    }

    pub fn get(&self, case_id: &str, file_id: &str) -> Option<HexSearchStatusDto> {
        let guard = self.lock();
        guard
            .entries
            .get(&(case_id.to_string(), file_id.to_string()))
            .map(|entry| entry.status.clone())
    }

    pub fn update_if_current(
        &self,
        case_id: &str,
        file_id: &str,
        task_id: &str,
        update: impl FnOnce(&mut HexSearchStatusDto),
    ) -> bool {
        let mut guard = self.lock();
        let state = &mut *guard;
        let key = (case_id.to_string(), file_id.to_string());
        let Some(entry) = state.entries.get_mut(&key) else {
            return false;
        };
        if entry.status.task_id != task_id {
            return false;
        }
        update(&mut entry.status);
        state.clock += 1;
        entry.updated = state.clock;
        true
    }

    pub fn mark_cancelling(&self, case_id: &str, file_id: &str, task_id: &str) -> bool {
        self.update_if_current(case_id, file_id, task_id, |status| {
            if !status.phase.is_terminal() {
                status.phase = HexSearchPhaseDto::Cancelling;
            }
        })
    }

    /// Resolve non-terminal entries whose task is no longer running.
    ///
    /// A task manager drops a task only after its worker returned, so a
    /// non-terminal entry with no running task means the terminal update was
    /// lost and the search must be surfaced as cancelled or interrupted.
    pub fn reconcile(
        &self,
        case_id: &str,
        file_id: &str,
        is_running: impl Fn(&str) -> bool,
    ) -> Option<HexSearchStatusDto> {
        let mut guard = self.lock();
        let state = &mut *guard;
        let key = (case_id.to_string(), file_id.to_string());
        let entry = state.entries.get_mut(&key)?;
        if !entry.status.phase.is_terminal() && !is_running(&entry.status.task_id) {
            if entry.status.phase == HexSearchPhaseDto::Cancelling {
                entry.status.phase = HexSearchPhaseDto::Cancelled;
            } else {
                entry.status.phase = HexSearchPhaseDto::Failed;
                entry.status.error = Some("task_interrupted".to_string());
            }
            state.clock += 1;
            entry.updated = state.clock;
        }
        Some(entry.status.clone())
    }
}

fn insert_entry(
    state: &mut RegistryState,
    key: (String, String),
    mode: HexSearchModeDto,
    pattern: &[u8],
    file_id: &str,
) -> Option<HexSearchStatusDto> {
    if !state.entries.contains_key(&key) && state.entries.len() >= MAX_HEX_SEARCH_JOBS {
        let evicted = state
            .entries
            .iter()
            .filter(|(_, entry)| entry.status.phase.is_terminal())
            .min_by_key(|(_, entry)| (entry.updated, entry.seq))
            .map(|(key, _)| key.clone());
        state.entries.remove(&evicted?);
    }
    let seq = state.next_seq;
    state.next_seq += 1;
    state.clock += 1;
    let status = HexSearchStatusDto {
        task_id: format!("hex-search:{}:{}:{}", key.0, key.1, seq),
        file_id: file_id.to_string(),
        phase: HexSearchPhaseDto::Queued,
        scanned_bytes: 0,
        total_bytes: 0,
        matches: Vec::new(),
        truncated: false,
        error: None,
    };
    state.entries.insert(
        key,
        Entry {
            status: status.clone(),
            mode,
            pattern: pattern.to_vec(),
            seq,
            updated: state.clock,
        },
    );
    Some(status)
}
