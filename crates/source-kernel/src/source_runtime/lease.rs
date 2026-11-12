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
    pool: Arc<Mutex<Option<crate::resource_budget::ResourcePool>>>,
}

impl std::fmt::Debug for SourcePublicationLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourcePublicationLease")
            .field("current", &self.validate_epoch().is_ok())
            .finish_non_exhaustive()
    }
}

impl SourcePublicationLease {
    pub fn acquire(case_id: &str, source_id: &str, factory: SharedReaderFactory) -> Self {
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
            pool: Arc::new(Mutex::new(None)),
        }
    }

    pub fn bind_pool(&self, pool: crate::resource_budget::ResourcePool) {
        self.factory
            .bind_reader_scope(Arc::new(super::resources::WorkspaceAdmission(pool.clone())));
        *self.pool.lock().unwrap_or_else(|p| p.into_inner()) = Some(pool);
    }
    pub fn resource_pool(&self) -> crate::resource_budget::ResourcePool {
        self.pool
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
            .unwrap_or_else(|| crate::resource_budget::global_pool().clone())
    }
    pub fn reader_memory_bound(&self) -> u64 {
        self.factory.reader_memory_bound()
    }
    pub fn validate(&self) -> io::Result<()> {
        self.validate_epoch()?;
        self.factory.validate_snapshot()?;
        self.validate_epoch()
    }

    pub fn validate_epoch(&self) -> io::Result<()> {
        if !self.owns_failure_state()
            || self.scope.revocation.load(Ordering::Acquire) != self.revocation
        {
            return Err(io::Error::other(SourceInvalidated));
        }
        Ok(())
    }

    /// A revoked last owner may record its failure; a superseded owner may not
    /// overwrite a newer result, even after that newer runtime has been dropped.
    pub fn owns_failure_state(&self) -> bool {
        self.scope.owner.load(Ordering::Acquire) == self.owner
    }

    pub fn failure_fence_connection<'a>(
        &self,
        connection: &'a rusqlite::Connection,
    ) -> SourcePublicationFence<'a> {
        SourcePublicationFence::install(connection, self.clone(), |lease| {
            !lease.owns_failure_state()
        })
    }

    /// Only for a connection owned by this operation, with no pre-existing hook.
    /// The commit hook must not query SQLite or emit application events.
    pub fn fence_connection<'a>(
        &self,
        connection: &'a rusqlite::Connection,
    ) -> SourcePublicationFence<'a> {
        SourcePublicationFence::install(connection, self.clone(), |lease| lease.validate().is_err())
    }
}

struct PublicationHook {
    lease: SourcePublicationLease,
    rollback: fn(&SourcePublicationLease) -> bool,
}

/// SQLite keeps one commit hook per connection. rusqlite 0.41 requires
/// `&mut Connection` to register one, but the fence is installed over shared
/// connections that stay borrowed by the caller for the publication window, so
/// the hook is registered directly. The fence owns the boxed hook and clears
/// the registration before freeing it.
// SAFETY: `data` is a live `Box<PublicationHook>` installed by
// `SourcePublicationFence::install`; SQLite invokes the callback on the
// connection's thread while the fence is alive.
unsafe extern "C" fn commit_hook_callback(data: *mut std::ffi::c_void) -> std::ffi::c_int {
    // SAFETY: installation keeps this pointer valid for the hook lifetime.
    let hook = unsafe { &*data.cast::<PublicationHook>() };
    // An unwind must never cross the SQLite C frame; rolling back is the safe
    // fallback if the predicate panics.
    let rollback = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        (hook.rollback)(&hook.lease)
    }))
    .unwrap_or(true);
    rollback as std::ffi::c_int
}

pub struct SourcePublicationFence<'a> {
    connection: &'a rusqlite::Connection,
    hook: *mut PublicationHook,
}

impl<'a> SourcePublicationFence<'a> {
    fn install(
        connection: &'a rusqlite::Connection,
        lease: SourcePublicationLease,
        rollback: fn(&SourcePublicationLease) -> bool,
    ) -> Self {
        let hook = Box::into_raw(Box::new(PublicationHook { lease, rollback }));
        // SAFETY: the SQLite handle is live for `'a`; the registration points at
        // `hook`, which this guard clears in Drop before freeing the allocation.
        unsafe {
            rusqlite::ffi::sqlite3_commit_hook(
                connection.handle(),
                Some(commit_hook_callback),
                hook.cast(),
            );
        }
        Self { connection, hook }
    }
}

impl Drop for SourcePublicationFence<'_> {
    fn drop(&mut self) {
        // SAFETY: Drop clears the registration installed above before freeing
        // the boxed hook; the connection is still live for `'a`.
        unsafe {
            rusqlite::ffi::sqlite3_commit_hook(
                self.connection.handle(),
                None,
                std::ptr::null_mut(),
            );
            drop(Box::from_raw(self.hook));
        }
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
