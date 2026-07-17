use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

use crate::file_service::FileServiceError;

use super::{
    bump_generation, invalidate_scope_locked, prune_runtime_lru, prune_session_lru,
    PreviewRuntimeRegistry, RegistryState, RuntimeKey,
};

impl PreviewRuntimeRegistry {
    pub fn invalidate_case(&self, case_id: &str) -> Result<(), FileServiceError> {
        let mut state = self.lock_state()?;
        invalidate_case_locked(&mut state, case_id);
        self.runtime_ready.notify_all();
        Ok(())
    }

    pub fn invalidate_source(
        &self,
        case_id: &str,
        data_source_id: &str,
    ) -> Result<(), FileServiceError> {
        let mut state = self.lock_state()?;
        let key = runtime_key(case_id, data_source_id);
        invalidate_scope_locked(&mut state, &key);
        self.runtime_ready.notify_all();
        Ok(())
    }

    pub fn retire_case_and_drain(
        &self,
        case_id: &str,
        timeout: Duration,
    ) -> Result<bool, FileServiceError> {
        let mut state = self.lock_state()?;
        state.retired_cases.insert(case_id.to_string());
        invalidate_case_locked(&mut state, case_id);
        self.runtime_ready.notify_all();
        self.wait_for_scope_drain(state, timeout, |key| key.case_id == case_id)
    }

    pub fn retire_source_and_drain(
        &self,
        case_id: &str,
        data_source_id: &str,
        timeout: Duration,
    ) -> Result<bool, FileServiceError> {
        let key = runtime_key(case_id, data_source_id);
        let mut state = self.lock_state()?;
        state.retired_sources.insert(key.clone());
        invalidate_scope_locked(&mut state, &key);
        self.runtime_ready.notify_all();
        self.wait_for_scope_drain(state, timeout, |candidate| candidate == &key)
    }

    pub fn reactivate_case(&self, case_id: &str) -> Result<(), FileServiceError> {
        let mut state = self.lock_state()?;
        state.retired_cases.remove(case_id);
        state
            .retired_sources
            .retain(|key| key.case_id.as_str() != case_id);
        Ok(())
    }

    pub fn reactivate_source(
        &self,
        case_id: &str,
        data_source_id: &str,
    ) -> Result<(), FileServiceError> {
        let mut state = self.lock_state()?;
        state
            .retired_sources
            .remove(&runtime_key(case_id, data_source_id));
        Ok(())
    }

    fn wait_for_scope_drain(
        &self,
        mut state: std::sync::MutexGuard<'_, RegistryState>,
        timeout: Duration,
        matches: impl Fn(&RuntimeKey) -> bool,
    ) -> Result<bool, FileServiceError> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .unwrap_or_else(Instant::now);
        loop {
            if !scope_is_busy(&state, &matches) {
                return Ok(true);
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(false);
            }
            let remaining = deadline.saturating_duration_since(now);
            let (next_state, wait) = self
                .runtime_ready
                .wait_timeout(state, remaining)
                .map_err(|_| FileServiceError::other("Preview runtime lock is poisoned"))?;
            state = next_state;
            if wait.timed_out() && scope_is_busy(&state, &matches) {
                return Ok(false);
            }
        }
    }
}

fn runtime_key(case_id: &str, data_source_id: &str) -> RuntimeKey {
    RuntimeKey {
        case_id: case_id.to_string(),
        data_source_id: data_source_id.to_string(),
    }
}

fn invalidate_case_locked(state: &mut RegistryState, case_id: &str) {
    let keys = state
        .generations
        .keys()
        .chain(state.runtimes.keys())
        .chain(state.building.iter())
        .filter(|key| key.case_id == case_id)
        .cloned()
        .collect::<HashSet<_>>();
    for key in keys {
        bump_generation(state, &key);
    }
    state
        .sessions
        .retain(|_, entry| entry.session.case_id() != case_id);
    prune_session_lru(state);
    state.runtimes.retain(|key, _| key.case_id != case_id);
    prune_runtime_lru(state);
}

fn scope_is_busy(state: &RegistryState, matches: &impl Fn(&RuntimeKey) -> bool) -> bool {
    state.building.iter().any(matches)
        || state
            .active_opens
            .iter()
            .any(|(key, active)| *active > 0 && matches(key))
        || state
            .active_leases
            .iter()
            .any(|(key, active)| *active > 0 && matches(key))
}
