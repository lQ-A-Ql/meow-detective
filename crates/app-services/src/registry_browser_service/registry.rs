use super::{session::Session, RegistryBrowserError};
use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard},
    time::{Duration, Instant},
};

pub(super) const TTL: Duration = Duration::from_secs(30 * 60);
pub(super) const MAX_HIVE_BYTES: usize = 128 * 1024 * 1024;
pub(super) const MAX_RETAINED_BYTES: usize = 256 * 1024 * 1024;
pub(super) const MAX_SESSIONS: usize = 8;
pub(super) const METADATA_BUDGET: usize = 16 * 1024 * 1024;

#[derive(Default)]
pub struct RegistryBrowserRegistry {
    pub(super) state: Mutex<RegistryState>,
}
#[derive(Default)]
pub(super) struct RegistryState {
    pub sessions: HashMap<String, Session>,
    pub reads: u64,
    pub log_reads: u64,
    pub discovery_cursors: HashMap<String, DiscoveryCursor>,
}
pub(super) struct DiscoveryCursor {
    pub case_id: String,
    pub source_id: String,
    pub revision: String,
    pub after: String,
    pub created: Instant,
}
#[derive(Debug, Clone, Copy)]
pub struct RegistryBrowserStats {
    pub session_count: usize,
    pub retained_bytes: usize,
    pub hive_reads: u64,
    pub log_reads: u64,
}
impl RegistryBrowserRegistry {
    pub(super) fn lock(&self) -> Result<MutexGuard<'_, RegistryState>, RegistryBrowserError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| RegistryBrowserError::Internal)?;
        state.sessions.retain(|_, s| s.last_used.elapsed() < TTL);
        state
            .discovery_cursors
            .retain(|_, cursor| cursor.created.elapsed() < TTL);
        Ok(state)
    }
    pub fn invalidate_case(&self, case_id: &str) -> Result<(), RegistryBrowserError> {
        let mut state = self.lock()?;
        state.sessions.retain(|_, s| s.source.case_id != case_id);
        state.discovery_cursors.retain(|_, c| c.case_id != case_id);
        Ok(())
    }
    pub fn invalidate_source(
        &self,
        case_id: &str,
        source_id: &str,
    ) -> Result<(), RegistryBrowserError> {
        let mut state = self.lock()?;
        state
            .sessions
            .retain(|_, s| s.source.case_id != case_id || s.source.data_source_id != source_id);
        state
            .discovery_cursors
            .retain(|_, c| c.case_id != case_id || c.source_id != source_id);
        Ok(())
    }
    pub fn stats(&self) -> Result<RegistryBrowserStats, RegistryBrowserError> {
        let s = self.lock()?;
        Ok(RegistryBrowserStats {
            session_count: s.sessions.len(),
            retained_bytes: s.bytes(),
            hive_reads: s.reads,
            log_reads: s.log_reads,
        })
    }
}
impl RegistryState {
    pub fn bytes(&self) -> usize {
        self.sessions
            .values()
            .map(|s| s.hive.retained_bytes() + METADATA_BUDGET)
            .sum()
    }
    pub fn reserve(&mut self, bytes: usize) -> Result<(), RegistryBrowserError> {
        // Opens and reads hold the mutex throughout. Evicted buffers have no
        // external leases, so the accounting includes all in-flight snapshots.
        while self.sessions.len() >= MAX_SESSIONS
            || self.bytes().saturating_add(bytes) > MAX_RETAINED_BYTES
        {
            let oldest = self
                .sessions
                .iter()
                .min_by_key(|(_, s)| s.last_used)
                .map(|(id, _)| id.clone())
                .ok_or(RegistryBrowserError::Busy)?;
            self.sessions.remove(&oldest);
        }
        Ok(())
    }
}
pub(super) fn touch(session: &mut Session) {
    session.last_used = Instant::now();
}
