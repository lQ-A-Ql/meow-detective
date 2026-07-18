use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

use domain::DataSourceId;

use crate::{
    ceph_reconstruction::{derived_catalog_fingerprint, DerivedRbdRuntime},
    file_service::{
        derived_filesystem::open_derived_filesystem,
        filesystem_locators::{derived_filesystem_locator_scope, restore_filesystem_locators},
        preview_runtime::prepared_ceph::SharedPreparedFilesystem,
        viewer::PreviewPartitionCandidate,
        FileServiceError,
    },
};

use super::{
    ensure_scope_token_locked, scope_is_retired, FilesystemEntry, FilesystemKey,
    PreviewRuntimeRegistry, PreviewScopeToken, RegistryState, RuntimeKey,
};

impl PreviewRuntimeRegistry {
    pub(crate) fn resolve_derived_filesystem(
        &self,
        source_conn: &rusqlite::Connection,
        data_source_id: &DataSourceId,
        runtime: &Arc<DerivedRbdRuntime>,
        candidate: &PreviewPartitionCandidate,
        token: &PreviewScopeToken<'_>,
    ) -> Result<SharedPreparedFilesystem, FileServiceError> {
        let key = filesystem_key(runtime, candidate, token)?;
        loop {
            let mut state = self.lock_state()?;
            self.cleanup_expired_locked(&mut state);
            ensure_scope_token_locked(&state, token)?;
            ensure_runtime_is_current(&state, &key)?;
            if let Some(filesystem) = matching_filesystem(&mut state, &key) {
                return Ok(filesystem);
            }
            if state.building_filesystems.insert(key.clone()) {
                break;
            }
            state = self
                .runtime_ready
                .wait(state)
                .map_err(|_| FileServiceError::other("Preview runtime lock is poisoned"))?;
            drop(state);
        }

        let built = build_shared_filesystem(source_conn, data_source_id, runtime, candidate);
        let mut state = self.lock_state()?;
        state.building_filesystems.remove(&key);
        let validation = ensure_scope_token_locked(&state, token)
            .and_then(|()| ensure_runtime_is_current(&state, &key));
        if let Err(error) = validation {
            self.runtime_ready.notify_all();
            return Err(error);
        }
        if let Ok(filesystem) = &built {
            self.filesystem_constructions
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            insert_filesystem_locked(&mut state, key, filesystem.clone(), self.max_filesystems);
        }
        self.runtime_ready.notify_all();
        built
    }
}

fn filesystem_key(
    runtime: &DerivedRbdRuntime,
    candidate: &PreviewPartitionCandidate,
    token: &PreviewScopeToken<'_>,
) -> Result<FilesystemKey, FileServiceError> {
    if runtime.data_source_id().0 != token.key.data_source_id {
        return Err(FileServiceError::security(
            "Preview filesystem scope does not match the derived runtime",
        ));
    }
    let candidate_identity = serde_json::to_string(candidate).map_err(|error| {
        FileServiceError::other(format!(
            "Preview filesystem candidate could not be serialized: {error}"
        ))
    })?;
    Ok(FilesystemKey {
        runtime: token.key.clone(),
        fingerprint: runtime.lineage_fingerprint().to_string(),
        candidate_identity,
    })
}

fn build_shared_filesystem(
    source_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    runtime: &DerivedRbdRuntime,
    candidate: &PreviewPartitionCandidate,
) -> Result<SharedPreparedFilesystem, FileServiceError> {
    let filesystem = open_derived_filesystem(runtime, candidate)?;
    let catalog_fingerprint = derived_catalog_fingerprint(runtime.lineage_fingerprint());
    let locator_scope = derived_filesystem_locator_scope(&catalog_fingerprint, candidate)?;
    restore_filesystem_locators(
        source_conn,
        data_source_id,
        candidate,
        &locator_scope,
        filesystem.as_ref(),
    );
    Ok(Arc::new(Mutex::new(filesystem)))
}

fn matching_filesystem(
    state: &mut RegistryState,
    key: &FilesystemKey,
) -> Option<SharedPreparedFilesystem> {
    let filesystem = state
        .filesystems
        .get(key)
        .map(|entry| entry.filesystem.clone())?;
    touch_filesystem(state, key);
    Some(filesystem)
}

fn ensure_runtime_is_current(
    state: &RegistryState,
    key: &FilesystemKey,
) -> Result<(), FileServiceError> {
    if scope_is_retired(state, &key.runtime) {
        return Err(FileServiceError::not_found(
            "Preview scope changed or is no longer available",
        ));
    }
    let is_current = state
        .runtimes
        .get(&key.runtime)
        .is_some_and(|entry| entry.runtime.lineage_fingerprint() == key.fingerprint);
    if !is_current {
        return Err(FileServiceError::not_found(
            "Preview derived runtime changed while opening its filesystem",
        ));
    }
    Ok(())
}

fn insert_filesystem_locked(
    state: &mut RegistryState,
    key: FilesystemKey,
    filesystem: SharedPreparedFilesystem,
    max_filesystems: usize,
) {
    state.filesystems.insert(
        key.clone(),
        FilesystemEntry {
            filesystem,
            last_used: Instant::now(),
        },
    );
    touch_filesystem(state, &key);
    evict_unused_filesystems(state, max_filesystems);
}

fn touch_filesystem(state: &mut RegistryState, key: &FilesystemKey) {
    if let Some(entry) = state.filesystems.get_mut(key) {
        entry.last_used = Instant::now();
    }
    state.filesystem_lru.retain(|candidate| candidate != key);
    state.filesystem_lru.push_back(key.clone());
}

fn evict_unused_filesystems(state: &mut RegistryState, max_filesystems: usize) {
    let mut remaining = state.filesystem_lru.len();
    while state.filesystems.len() > max_filesystems && remaining > 0 {
        remaining -= 1;
        let Some(oldest) = state.filesystem_lru.pop_front() else {
            break;
        };
        let in_use = state
            .filesystems
            .get(&oldest)
            .is_some_and(|entry| Arc::strong_count(&entry.filesystem) > 1);
        if in_use {
            state.filesystem_lru.push_back(oldest);
        } else {
            state.filesystems.remove(&oldest);
        }
    }
}

pub(super) fn invalidate_filesystems_for_runtime(
    state: &mut RegistryState,
    runtime_key: &RuntimeKey,
) {
    state
        .filesystems
        .retain(|key, _| &key.runtime != runtime_key);
    state
        .filesystem_lru
        .retain(|key| &key.runtime != runtime_key);
}

#[cfg(test)]
#[path = "../../../../tests/unit/file_service/preview_runtime/registry/filesystem.rs"]
mod tests;
