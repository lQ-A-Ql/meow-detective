use std::{
    collections::{HashMap, HashSet, VecDeque},
    ops::Deref,
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};

use domain::{CaseId, DataSourceId};

use crate::{
    ceph_reconstruction::{build_derived_rbd_runtime, load_lineage_fingerprint, DerivedRbdRuntime},
    file_service::{
        preview_runtime::{prepared_ceph::SharedPreparedFilesystem, session::PreviewSession},
        FileServiceError,
    },
};

mod filesystem;
mod lifecycle;

const DEFAULT_SESSION_TTL: Duration = Duration::from_secs(30 * 60);
const DEFAULT_MAX_SESSIONS: usize = 32;
const DEFAULT_MAX_RUNTIMES: usize = 1;
const MAX_FILESYSTEMS_PER_RUNTIME: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewRuntimeStats {
    pub runtime_count: usize,
    pub filesystem_count: usize,
    pub session_count: usize,
    pub provider_constructions: u64,
    pub filesystem_constructions: u64,
    pub runtime_cache_capacity_bytes: usize,
    pub max_sessions: usize,
    pub max_runtimes: usize,
    pub max_filesystems: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct RuntimeKey {
    case_id: String,
    data_source_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct FilesystemKey {
    runtime: RuntimeKey,
    fingerprint: String,
    candidate_identity: String,
}

struct RuntimeEntry {
    runtime: Arc<DerivedRbdRuntime>,
    last_used: Instant,
}

pub(super) struct FilesystemEntry {
    filesystem: SharedPreparedFilesystem,
    last_used: Instant,
}

struct SessionEntry {
    session: Arc<PreviewSession>,
    last_used: Instant,
}

pub(crate) struct PreviewScopeToken<'a> {
    registry: &'a PreviewRuntimeRegistry,
    key: RuntimeKey,
    generation: u64,
}

impl Drop for PreviewScopeToken<'_> {
    fn drop(&mut self) {
        self.registry.release_open(&self.key);
    }
}

pub(crate) struct PreviewSessionLease<'a> {
    registry: &'a PreviewRuntimeRegistry,
    key: RuntimeKey,
    session: Arc<PreviewSession>,
}

impl Deref for PreviewSessionLease<'_> {
    type Target = PreviewSession;

    fn deref(&self) -> &Self::Target {
        &self.session
    }
}

impl Drop for PreviewSessionLease<'_> {
    fn drop(&mut self) {
        self.registry.release_lease(&self.key);
    }
}

#[derive(Default)]
pub(super) struct RegistryState {
    runtimes: HashMap<RuntimeKey, RuntimeEntry>,
    runtime_lru: VecDeque<RuntimeKey>,
    building: HashSet<RuntimeKey>,
    filesystems: HashMap<FilesystemKey, FilesystemEntry>,
    filesystem_lru: VecDeque<FilesystemKey>,
    building_filesystems: HashSet<FilesystemKey>,
    sessions: HashMap<String, SessionEntry>,
    session_lru: VecDeque<String>,
    generations: HashMap<RuntimeKey, u64>,
    retired_cases: HashSet<String>,
    retired_sources: HashSet<RuntimeKey>,
    active_opens: HashMap<RuntimeKey, usize>,
    active_leases: HashMap<RuntimeKey, usize>,
}

pub struct PreviewRuntimeRegistry {
    state: Mutex<RegistryState>,
    runtime_ready: Condvar,
    session_ttl: Duration,
    max_sessions: usize,
    max_runtimes: usize,
    max_filesystems: usize,
    provider_constructions: AtomicU64,
    filesystem_constructions: AtomicU64,
}

impl Default for PreviewRuntimeRegistry {
    fn default() -> Self {
        Self::new(
            DEFAULT_SESSION_TTL,
            DEFAULT_MAX_SESSIONS,
            DEFAULT_MAX_RUNTIMES,
        )
    }
}

impl PreviewRuntimeRegistry {
    pub fn new(session_ttl: Duration, max_sessions: usize, max_runtimes: usize) -> Self {
        Self {
            state: Mutex::new(RegistryState::default()),
            runtime_ready: Condvar::new(),
            session_ttl,
            max_sessions: max_sessions.max(1),
            max_runtimes: max_runtimes.max(1),
            max_filesystems: max_runtimes
                .max(1)
                .saturating_mul(MAX_FILESYSTEMS_PER_RUNTIME),
            provider_constructions: AtomicU64::new(0),
            filesystem_constructions: AtomicU64::new(0),
        }
    }

    pub(crate) fn resolve_derived_runtime(
        &self,
        case_conn: &rusqlite::Connection,
        case_root: &Path,
        case_id: &CaseId,
        data_source_id: &DataSourceId,
        token: &PreviewScopeToken<'_>,
    ) -> Result<Arc<DerivedRbdRuntime>, FileServiceError> {
        let fingerprint = load_lineage_fingerprint(case_conn, data_source_id)
            .map_err(|error| FileServiceError::other(error.to_string()))?;
        let key = RuntimeKey {
            case_id: case_id.0.clone(),
            data_source_id: data_source_id.0.clone(),
        };
        if token.key != key {
            return Err(FileServiceError::security(
                "Preview scope does not match the requested data source",
            ));
        }

        loop {
            let mut state = self.lock_state()?;
            self.cleanup_expired_locked(&mut state);
            ensure_scope_token_locked(&state, token)?;
            if let Some(runtime) = matching_runtime(&mut state, &key, &fingerprint) {
                touch_runtime(&mut state, &key);
                return Ok(runtime);
            }
            invalidate_runtime_locked(&mut state, &key);
            if state.building.insert(key.clone()) {
                break;
            }
            state = self
                .runtime_ready
                .wait(state)
                .map_err(|_| FileServiceError::other("Preview runtime lock is poisoned"))?;
            drop(state);
        }

        let built = build_derived_rbd_runtime(case_conn, case_root, case_id, data_source_id)
            .map(Arc::new)
            .map_err(|error| FileServiceError::other(error.to_string()));
        let mut state = self.lock_state()?;
        state.building.remove(&key);
        if let Err(error) = ensure_scope_token_locked(&state, token) {
            self.runtime_ready.notify_all();
            return Err(error);
        }
        if let Ok(runtime) = &built {
            self.provider_constructions.fetch_add(1, Ordering::Relaxed);
            insert_runtime_locked(&mut state, key, runtime.clone(), self.max_runtimes);
        }
        self.runtime_ready.notify_all();
        built
    }

    pub(crate) fn begin_session<'a>(
        &'a self,
        case_id: &CaseId,
        data_source_id: &DataSourceId,
    ) -> Result<PreviewScopeToken<'a>, FileServiceError> {
        let key = RuntimeKey {
            case_id: case_id.0.clone(),
            data_source_id: data_source_id.0.clone(),
        };
        let mut state = self.lock_state()?;
        if scope_is_retired(&state, &key) {
            return Err(preview_scope_unavailable());
        }
        let generation = *state.generations.entry(key.clone()).or_default();
        *state.active_opens.entry(key.clone()).or_default() += 1;
        Ok(PreviewScopeToken {
            registry: self,
            key,
            generation,
        })
    }

    pub(crate) fn insert_session(
        &self,
        token: &PreviewScopeToken<'_>,
        session: PreviewSession,
    ) -> Result<String, FileServiceError> {
        if session.case_id() != token.key.case_id
            || session.data_source_id() != token.key.data_source_id
        {
            return Err(FileServiceError::security(
                "Preview session scope does not match its token",
            ));
        }
        let handle_id = format!("preview:{}", uuid::Uuid::new_v4().as_simple());
        let mut state = self.lock_state()?;
        self.cleanup_expired_locked(&mut state);
        ensure_scope_token_locked(&state, token)?;
        state.sessions.insert(
            handle_id.clone(),
            SessionEntry {
                session: Arc::new(session),
                last_used: Instant::now(),
            },
        );
        touch_session(&mut state, &handle_id);
        while state.sessions.len() > self.max_sessions {
            evict_oldest_session(&mut state);
        }
        Ok(handle_id)
    }

    pub(crate) fn get_session(
        &self,
        case_id: &str,
        handle_id: &str,
    ) -> Result<PreviewSessionLease<'_>, FileServiceError> {
        let mut state = self.lock_state()?;
        self.cleanup_expired_locked(&mut state);
        let session = state
            .sessions
            .get(handle_id)
            .map(|entry| entry.session.clone())
            .ok_or_else(|| FileServiceError::not_found("Preview handle expired or invalid"))?;
        if session.case_id() != case_id {
            return Err(FileServiceError::not_found(
                "Preview handle expired or invalid",
            ));
        }
        let key = RuntimeKey {
            case_id: session.case_id().to_string(),
            data_source_id: session.data_source_id().to_string(),
        };
        if scope_is_retired(&state, &key) {
            return Err(preview_scope_unavailable());
        }
        if let Some(entry) = state.sessions.get_mut(handle_id) {
            entry.last_used = Instant::now();
        }
        *state.active_leases.entry(key.clone()).or_default() += 1;
        touch_session(&mut state, handle_id);
        Ok(PreviewSessionLease {
            registry: self,
            key,
            session,
        })
    }

    pub fn close_session(&self, case_id: &str, handle_id: &str) -> Result<bool, FileServiceError> {
        let mut state = self.lock_state()?;
        let matches_case = state
            .sessions
            .get(handle_id)
            .is_some_and(|entry| entry.session.case_id() == case_id);
        if matches_case {
            state.sessions.remove(handle_id);
            state.session_lru.retain(|candidate| candidate != handle_id);
        }
        Ok(matches_case)
    }

    pub fn provider_construction_count(&self) -> u64 {
        self.provider_constructions.load(Ordering::Relaxed)
    }

    pub fn stats(&self) -> Result<PreviewRuntimeStats, FileServiceError> {
        let mut state = self.lock_state()?;
        self.cleanup_expired_locked(&mut state);
        Ok(PreviewRuntimeStats {
            runtime_count: state.runtimes.len(),
            filesystem_count: state.filesystems.len(),
            session_count: state.sessions.len(),
            provider_constructions: self.provider_construction_count(),
            filesystem_constructions: self.filesystem_constructions.load(Ordering::Relaxed),
            runtime_cache_capacity_bytes: state
                .runtimes
                .values()
                .map(|entry| entry.runtime.cache_capacity_bytes())
                .sum(),
            max_sessions: self.max_sessions,
            max_runtimes: self.max_runtimes,
            max_filesystems: self.max_filesystems,
        })
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, RegistryState>, FileServiceError> {
        self.state
            .lock()
            .map_err(|_| FileServiceError::other("Preview runtime lock is poisoned"))
    }

    fn cleanup_expired_locked(&self, state: &mut RegistryState) {
        let cutoff = Instant::now()
            .checked_sub(self.session_ttl)
            .unwrap_or_else(Instant::now);
        state.sessions.retain(|_, entry| entry.last_used >= cutoff);
        prune_session_lru(state);
    }

    fn release_lease(&self, key: &RuntimeKey) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(active) = state.active_leases.get_mut(key) {
            *active = active.saturating_sub(1);
            if *active == 0 {
                state.active_leases.remove(key);
            }
        }
        self.runtime_ready.notify_all();
    }

    fn release_open(&self, key: &RuntimeKey) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(active) = state.active_opens.get_mut(key) {
            *active = active.saturating_sub(1);
            if *active == 0 {
                state.active_opens.remove(key);
            }
        }
        self.runtime_ready.notify_all();
    }
}

fn matching_runtime(
    state: &mut RegistryState,
    key: &RuntimeKey,
    fingerprint: &str,
) -> Option<Arc<DerivedRbdRuntime>> {
    state.runtimes.get(key).and_then(|entry| {
        (entry.runtime.lineage_fingerprint() == fingerprint).then(|| entry.runtime.clone())
    })
}

fn insert_runtime_locked(
    state: &mut RegistryState,
    key: RuntimeKey,
    runtime: Arc<DerivedRbdRuntime>,
    max_runtimes: usize,
) {
    state.runtimes.insert(
        key.clone(),
        RuntimeEntry {
            runtime,
            last_used: Instant::now(),
        },
    );
    touch_runtime(state, &key);
    while state.runtimes.len() > max_runtimes {
        let Some(oldest) = state.runtime_lru.pop_front() else {
            break;
        };
        if oldest == key {
            state.runtime_lru.push_back(oldest);
            continue;
        }
        invalidate_scope_locked(state, &oldest);
    }
}

pub(super) fn invalidate_scope_locked(state: &mut RegistryState, key: &RuntimeKey) {
    bump_generation(state, key);
    invalidate_runtime_locked(state, key);
    state.sessions.retain(|_, entry| {
        entry.session.case_id() != key.case_id
            || entry.session.data_source_id() != key.data_source_id
    });
    prune_session_lru(state);
}

fn invalidate_runtime_locked(state: &mut RegistryState, key: &RuntimeKey) {
    let fingerprint = state
        .runtimes
        .remove(key)
        .map(|entry| entry.runtime.lineage_fingerprint().to_string());
    state.runtime_lru.retain(|candidate| candidate != key);
    filesystem::invalidate_filesystems_for_runtime(state, key);
    if let Some(fingerprint) = fingerprint {
        state.sessions.retain(|_, entry| {
            entry.session.runtime_fingerprint() != Some(fingerprint.as_str())
                || entry.session.case_id() != key.case_id
                || entry.session.data_source_id() != key.data_source_id
        });
        prune_session_lru(state);
    }
}

fn bump_generation(state: &mut RegistryState, key: &RuntimeKey) {
    let generation = state.generations.entry(key.clone()).or_default();
    *generation = generation.wrapping_add(1);
}

fn ensure_scope_token_locked(
    state: &RegistryState,
    token: &PreviewScopeToken<'_>,
) -> Result<(), FileServiceError> {
    let generation = state.generations.get(&token.key).copied().unwrap_or(0);
    if generation != token.generation || scope_is_retired(state, &token.key) {
        return Err(preview_scope_unavailable());
    }
    Ok(())
}

fn preview_scope_unavailable() -> FileServiceError {
    FileServiceError::not_found("Preview scope changed or is no longer available")
}

fn scope_is_retired(state: &RegistryState, key: &RuntimeKey) -> bool {
    state.retired_cases.contains(&key.case_id) || state.retired_sources.contains(key)
}

fn touch_runtime(state: &mut RegistryState, key: &RuntimeKey) {
    if let Some(entry) = state.runtimes.get_mut(key) {
        entry.last_used = Instant::now();
    }
    state.runtime_lru.retain(|candidate| candidate != key);
    state.runtime_lru.push_back(key.clone());
}

fn touch_session(state: &mut RegistryState, handle_id: &str) {
    state.session_lru.retain(|candidate| candidate != handle_id);
    state.session_lru.push_back(handle_id.to_string());
}

fn evict_oldest_session(state: &mut RegistryState) {
    if let Some(handle_id) = state.session_lru.pop_front() {
        state.sessions.remove(&handle_id);
    }
}

fn prune_session_lru(state: &mut RegistryState) {
    let live = state.sessions.keys().cloned().collect::<HashSet<_>>();
    state.session_lru.retain(|handle| live.contains(handle));
}

fn prune_runtime_lru(state: &mut RegistryState) {
    let live = state.runtimes.keys().cloned().collect::<HashSet<_>>();
    state.runtime_lru.retain(|key| live.contains(key));
}
