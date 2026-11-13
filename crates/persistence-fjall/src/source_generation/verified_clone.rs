use super::{
    digest::hash_entry, read_source_generation_manifest, validate_identity, write_manifest,
    SourceGenerationManifest, SEALED_KEYSPACES, SOURCE_GENERATION_MANIFEST_FILE,
    SOURCE_GENERATION_SEAL_FILE,
};
use crate::{
    source_store::{ensure_not_cancelled, read_bounded_page},
    DatabaseConfig, FjallStorageError, PersistMode, Result, SourceCloneProgress,
    SourceReadSnapshot, SourceStore, WriteAction,
};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc, Weak},
};

pub struct VerifiedGenerationClone {
    target: Weak<crate::database::DatabaseState>,
    immutable_epoch: u64,
    manifest: SourceGenerationManifest,
}

impl VerifiedGenerationClone {
    pub(crate) fn validate_unpublished_target(
        &self,
        store: &SourceStore,
        source_id: &str,
    ) -> Result<()> {
        if self.manifest.source_id != source_id {
            return Err(FjallStorageError::InvalidEnvelope(
                "verified clone belongs to another source".into(),
            ));
        }
        if !Weak::ptr_eq(&self.target, &Arc::downgrade(&store.database().inner)) {
            return Err(FjallStorageError::InvalidEnvelope(
                "verified clone belongs to another target".into(),
            ));
        }
        if store
            .database()
            .path()
            .join(SOURCE_GENERATION_MANIFEST_FILE)
            .exists()
            || store
                .database()
                .path()
                .join(SOURCE_GENERATION_SEAL_FILE)
                .exists()
        {
            return Err(FjallStorageError::PublicationConflict(
                store.database().path().to_owned(),
            ));
        }
        Ok(())
    }

    pub fn seal(self, store: &SourceStore, attempt: &str, cancel: &AtomicBool) -> Result<String> {
        ensure_not_cancelled(cancel)?;
        validate_identity(attempt)?;
        if !Weak::ptr_eq(&self.target, &Arc::downgrade(&store.database().inner)) {
            return Err(FjallStorageError::InvalidEnvelope(
                "verified clone belongs to another target".into(),
            ));
        }
        if store
            .database()
            .path()
            .join(SOURCE_GENERATION_MANIFEST_FILE)
            .exists()
            || store
                .database()
                .path()
                .join(SOURCE_GENERATION_SEAL_FILE)
                .exists()
        {
            return Err(FjallStorageError::PublicationConflict(
                store.database().path().to_owned(),
            ));
        }
        let manifest = SourceGenerationManifest {
            attempt_id: attempt.to_owned(),
            ..self.manifest
        };
        store.database().seal_immutable(self.immutable_epoch, || {
            ensure_not_cancelled(cancel)?;
            write_manifest(store, &manifest, cancel)
        })
    }
}

pub fn clone_verified_generation_with_control(
    source: &SourceStore,
    expected_source: &str,
    path: impl AsRef<Path>,
    config: DatabaseConfig,
    cancel: &AtomicBool,
    mut progress: impl FnMut(SourceCloneProgress),
) -> Result<(SourceStore, VerifiedGenerationClone)> {
    ensure_not_cancelled(cancel)?;
    let manifest = read_source_generation_manifest(source, expected_source)?;
    let path = path.as_ref();
    if path.exists() && std::fs::read_dir(path)?.next().is_some() {
        return Err(FjallStorageError::PublicationConflict(path.to_owned()));
    }
    let target = SourceStore::open(path, config)?;
    let snapshot = source.database().read_snapshot();
    let mut remaining = source
        .database()
        .keyspace_names()
        .filter(|name| !SEALED_KEYSPACES.contains(name))
        .collect::<Vec<_>>();
    remaining.sort_unstable_by_key(|name| name.as_str());
    let spaces = SEALED_KEYSPACES
        .into_iter()
        .chain(remaining)
        .collect::<Vec<_>>();
    let mut state = SourceCloneProgress {
        keyspace: None,
        completed_keyspaces: 0,
        total_keyspaces: spaces.len(),
        records_copied: 0,
        bytes_copied: 0,
        durable: false,
    };
    let mut hasher = Sha256::new();
    progress(state);
    for space in spaces {
        let immutable = SEALED_KEYSPACES.contains(&space);
        if immutable {
            let name = space.as_str().as_bytes();
            hasher.update((name.len() as u64).to_be_bytes());
            hasher.update(name);
        }
        state.keyspace = Some(space);
        copy_space(
            &snapshot,
            &target,
            cancel,
            &mut state,
            &mut progress,
            immutable.then_some(&mut hasher),
        )?;
        state.completed_keyspaces += 1;
        progress(state);
    }
    let actual = hex::encode(hasher.finalize());
    if actual != manifest.immutable_data_digest {
        return Err(FjallStorageError::ManifestDigestMismatch {
            expected: manifest.immutable_data_digest,
            actual,
        });
    }
    ensure_not_cancelled(cancel)?;
    target.database().persist(PersistMode::SyncAll)?;
    state.keyspace = None;
    state.durable = true;
    progress(state);
    let verified = VerifiedGenerationClone {
        target: Arc::downgrade(&target.database().inner),
        immutable_epoch: target.database().immutable_epoch(),
        manifest,
    };
    Ok((target, verified))
}

fn copy_space(
    snapshot: &SourceReadSnapshot,
    target: &SourceStore,
    cancel: &AtomicBool,
    state: &mut SourceCloneProgress,
    progress: &mut impl FnMut(SourceCloneProgress),
    mut hasher: Option<&mut Sha256>,
) -> Result<()> {
    let space = state.keyspace.expect("copy keyspace is selected");
    let mut cursor = None;
    loop {
        ensure_not_cancelled(cancel)?;
        let (page, bytes) = read_bounded_page(snapshot, space, cursor.as_deref(), cancel)?;
        if page.is_empty() {
            return Ok(());
        }
        if let Some(hasher) = hasher.as_deref_mut() {
            for entry in &page {
                hash_entry(hasher, entry, cancel)?;
            }
        }
        let count = page.len() as u64;
        cursor = page.last().map(|entry| entry.key.clone());
        let actions = page
            .into_iter()
            .map(|entry| WriteAction::Put {
                keyspace: space,
                key: entry.key,
                value: entry.value,
            })
            .collect::<Vec<_>>();
        target
            .database()
            .write_batch(&actions, PersistMode::Buffer)?;
        state.records_copied = state
            .records_copied
            .checked_add(count)
            .ok_or_else(overflow)?;
        state.bytes_copied = state.bytes_copied.checked_add(bytes).ok_or_else(overflow)?;
        progress(*state);
    }
}

fn overflow() -> FjallStorageError {
    FjallStorageError::InvalidEnvelope("verified clone progress overflowed".into())
}
