//! Nonblocking source epochs, checked again at SQLite transaction publication.
use evidence_core::SharedReaderFactory;
use std::{
    collections::HashMap,
    io,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, LazyLock, Mutex, Weak,
    },
};

type Key = (String, String);
static EPOCHS: LazyLock<Mutex<HashMap<Key, Weak<ScopeEpoch>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Default)]
struct ScopeEpoch {
    owner: AtomicU64,
    revocation: AtomicU64,
}

#[derive(Debug, thiserror::Error)]
#[error("Evidence source scope changed or is no longer available")]
pub struct SourceInvalidated;

#[derive(Clone)]
pub struct SourcePublicationLease {
    scope: Arc<ScopeEpoch>,
    owner: u64,
    revocation: u64,
    factory: SharedReaderFactory,
}

impl std::fmt::Debug for SourcePublicationLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourcePublicationLease")
            .field("current", &self.validate_epoch().is_ok())
            .finish_non_exhaustive()
    }
}

impl SourcePublicationLease {
    pub(super) fn acquire(case_id: &str, source_id: &str, factory: SharedReaderFactory) -> Self {
        let mut epochs = EPOCHS.lock().unwrap_or_else(|p| p.into_inner());
        epochs.retain(|_, value| value.strong_count() > 0);
        let key = (case_id.into(), source_id.into());
        let scope = epochs.get(&key).and_then(Weak::upgrade).unwrap_or_default();
        let owner = scope.owner.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        let revocation = scope.revocation.load(Ordering::Acquire);
        epochs.insert(key, Arc::downgrade(&scope));
        Self {
            scope,
            owner,
            revocation,
            factory,
        }
    }

    pub fn validate(&self) -> io::Result<()> {
        self.validate_epoch()?;
        self.factory.validate_snapshot()?;
        self.validate_epoch()
    }

    pub(super) fn validate_epoch(&self) -> io::Result<()> {
        if !self.owns_failure_state()
            || self.scope.revocation.load(Ordering::Acquire) != self.revocation
        {
            return Err(io::Error::other(SourceInvalidated));
        }
        Ok(())
    }

    /// A revoked last owner may record its failure; a superseded owner may not
    /// overwrite a newer result, even after that newer runtime has been dropped.
    pub(crate) fn owns_failure_state(&self) -> bool {
        self.scope.owner.load(Ordering::Acquire) == self.owner
    }

    pub(crate) fn failure_fence_connection<'a>(
        &self,
        connection: &'a rusqlite::Connection,
    ) -> SourcePublicationFence<'a> {
        let lease = self.clone();
        connection.commit_hook(Some(move || !lease.owns_failure_state()));
        SourcePublicationFence(connection)
    }

    /// Only for a connection owned by this operation, with no pre-existing hook.
    /// The commit hook must not query SQLite or emit application events.
    pub(crate) fn fence_connection<'a>(
        &self,
        connection: &'a rusqlite::Connection,
    ) -> SourcePublicationFence<'a> {
        let lease = self.clone();
        connection.commit_hook(Some(move || lease.validate().is_err()));
        SourcePublicationFence(connection)
    }
}

pub(crate) struct SourcePublicationFence<'a>(&'a rusqlite::Connection);
impl Drop for SourcePublicationFence<'_> {
    fn drop(&mut self) {
        self.0.commit_hook(None::<fn() -> bool>);
    }
}

fn invalidate_matching(matches: impl Fn(&Key) -> bool) {
    let mut epochs = EPOCHS.lock().unwrap_or_else(|p| p.into_inner());
    epochs.retain(|key, value| {
        if let Some(current) = value.upgrade() {
            if matches(key) {
                current.revocation.fetch_add(1, Ordering::AcqRel);
            }
            true
        } else {
            false
        }
    });
}

pub fn invalidate_source(case_id: &str, source_id: &str) {
    invalidate_matching(|key| key.0 == case_id && key.1 == source_id);
}

pub fn invalidate_case(case_id: &str) {
    invalidate_matching(|key| key.0 == case_id);
}

#[cfg(test)]
#[path = "../../tests/unit/source_runtime_lease.rs"]
mod tests;
