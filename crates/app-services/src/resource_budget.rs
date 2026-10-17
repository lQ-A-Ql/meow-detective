//! Process-wide admission for estimated resident allocations and finite resources.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, LazyLock, Mutex};
use std::time::Duration;

pub const MIB: u64 = 1024 * 1024;
pub const ENUMERATION_BYTES: u64 = 560 * MIB;
pub const TIMELINE_BYTES: u64 = 1_000_000 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Resources {
    pub bytes: u64,
    pub readers: u64,
    pub handles: u64,
    pub workers: u64,
    pub metadata: u64,
    pub catalogs: u64,
}
impl Resources {
    fn fits(self, limit: Self) -> bool {
        self.bytes <= limit.bytes
            && self.readers <= limit.readers
            && self.handles <= limit.handles
            && self.workers <= limit.workers
            && self.metadata <= limit.metadata
            && self.catalogs <= limit.catalogs
    }
    fn plus(self, rhs: Self) -> Self {
        Self {
            bytes: self.bytes.saturating_add(rhs.bytes),
            readers: self.readers.saturating_add(rhs.readers),
            handles: self.handles.saturating_add(rhs.handles),
            workers: self.workers.saturating_add(rhs.workers),
            metadata: self.metadata.saturating_add(rhs.metadata),
            catalogs: self.catalogs.saturating_add(rhs.catalogs),
        }
    }
    fn minus(self, rhs: Self) -> Self {
        Self {
            bytes: self.bytes - rhs.bytes,
            readers: self.readers - rhs.readers,
            handles: self.handles - rhs.handles,
            workers: self.workers - rhs.workers,
            metadata: self.metadata - rhs.metadata,
            catalogs: self.catalogs - rhs.catalogs,
        }
    }
    fn maximum(self, rhs: Self) -> Self {
        Self {
            bytes: self.bytes.max(rhs.bytes),
            readers: self.readers.max(rhs.readers),
            handles: self.handles.max(rhs.handles),
            workers: self.workers.max(rhs.workers),
            metadata: self.metadata.max(rhs.metadata),
            catalogs: self.catalogs.max(rhs.catalogs),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ResourceError {
    #[error("Evidence allocation exceeds the process resource budget")]
    Limit,
    #[error("Evidence resources are busy")]
    Busy,
    #[error("Evidence resource wait cancelled")]
    Cancelled,
}
impl ResourceError {
    pub fn into_io(self) -> std::io::Error {
        let kind = match self {
            Self::Busy => std::io::ErrorKind::WouldBlock,
            Self::Cancelled => std::io::ErrorKind::Interrupted,
            Self::Limit => std::io::ErrorKind::OutOfMemory,
        };
        std::io::Error::new(kind, self)
    }
}
impl transport::ServiceErrorCategory for ResourceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Cancelled => transport::ErrorCategory::Cancelled,
            Self::Busy => transport::ErrorCategory::Io,
            Self::Limit => transport::ErrorCategory::Validation,
        }
    }
}

#[derive(Default)]
struct State {
    current: Resources,
    peak: Resources,
}
struct Inner {
    limits: Resources,
    state: Mutex<State>,
    changed: Condvar,
    parent: Option<Mutex<ResourcePermit>>,
    retired: AtomicBool,
}
#[derive(Clone)]
pub struct ResourcePool(Arc<Inner>);
pub struct ResourcePermit {
    pool: ResourcePool,
    request: Resources,
}
impl ResourcePool {
    pub fn new(limits: Resources) -> Self {
        Self(Arc::new(Inner {
            limits,
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
            parent: None,
            retired: AtomicBool::new(false),
        }))
    }
    pub fn limits(&self) -> Resources {
        self.0.limits
    }
    pub fn snapshot(&self) -> (Resources, Resources) {
        let state = self.0.state.lock().unwrap_or_else(|p| p.into_inner());
        (state.current, state.peak)
    }
    /// None is a nonblocking request, used while holding preview/cache locks.
    pub fn acquire(
        &self,
        request: Resources,
        cancel: Option<&AtomicBool>,
    ) -> Result<ResourcePermit, ResourceError> {
        if !request.fits(self.0.limits) {
            return Err(ResourceError::Limit);
        }
        let mut state = self.0.state.lock().unwrap_or_else(|p| p.into_inner());
        loop {
            if self.0.retired.load(Ordering::Acquire) {
                return Err(ResourceError::Busy);
            }
            if cancel.is_some_and(|c| c.load(Ordering::Acquire)) {
                return Err(ResourceError::Cancelled);
            }
            let next = state.current.plus(request);
            if next.fits(self.0.limits) {
                state.current = next;
                state.peak = state.peak.maximum(next);
                return Ok(ResourcePermit {
                    pool: self.clone(),
                    request,
                });
            }
            if cancel.is_none() || self.0.parent.is_some() {
                return Err(ResourceError::Busy);
            }
            state = self
                .0
                .changed
                .wait_timeout(state, Duration::from_millis(50))
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
    }
}
impl ResourcePermit {
    /// Transfer a parsing reservation to retained metadata without acquiring
    /// additional capacity while holding the larger parsing reservation.
    pub fn shrink(mut self, request: Resources) -> Result<Self, ResourceError> {
        if !request.fits(self.request) {
            return Err(ResourceError::Limit);
        }
        {
            let mut state = self.pool.0.state.lock().unwrap_or_else(|p| p.into_inner());
            state.current = state.current.minus(self.request).plus(request);
            self.pool.update_parent(&state);
        }
        self.request = request;
        self.pool.0.changed.notify_all();
        Ok(self)
    }
}
impl Drop for ResourcePermit {
    fn drop(&mut self) {
        let mut state = self.pool.0.state.lock().unwrap_or_else(|p| p.into_inner());
        state.current = state.current.minus(self.request);
        self.pool.update_parent(&state);
        self.pool.0.changed.notify_all();
    }
}

impl ResourcePool {
    /// Admit the complete import before allocating any phase working sets.
    pub fn workspace(&self, cancel: &AtomicBool) -> Result<Workspace, ResourceError> {
        let parent = self.acquire(
            Resources {
                bytes: 1024 * MIB,
                workers: 4,
                ..Default::default()
            },
            Some(cancel),
        )?;
        let pool = Self(Arc::new(Inner {
            limits: Resources {
                bytes: 1024 * MIB,
                workers: 4,
                catalogs: 1,
                readers: 32,
                handles: 512,
                metadata: 0,
            },
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
            parent: Some(Mutex::new(parent)),
            retired: AtomicBool::new(false),
        }));
        Ok(Workspace(pool))
    }
    pub fn active(&self) -> bool {
        !self.0.retired.load(Ordering::Acquire)
    }
    fn update_parent(&self, state: &State) {
        if !self.active() {
            if let Some(parent) = &self.0.parent {
                let mut parent = parent.lock().unwrap_or_else(|p| p.into_inner());
                let remaining = Resources {
                    bytes: state.current.bytes,
                    workers: state.current.workers,
                    ..Default::default()
                };
                let mut root = parent
                    .pool
                    .0
                    .state
                    .lock()
                    .unwrap_or_else(|p| p.into_inner());
                root.current = root.current.minus(parent.request).plus(remaining);
                drop(root);
                parent.request = remaining;
                parent.pool.0.changed.notify_all();
            }
        }
    }
}

pub struct Workspace(ResourcePool);
impl Workspace {
    pub fn pool(&self) -> ResourcePool {
        self.0.clone()
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let state = self.0 .0.state.lock().unwrap_or_else(|p| p.into_inner());
        self.0 .0.retired.store(true, Ordering::Release);
        self.0.update_parent(&state);
        self.0 .0.changed.notify_all();
    }
}
pub(crate) fn pool_for(
    lease: Option<&crate::source_runtime::SourcePublicationLease>,
) -> ResourcePool {
    lease
        .map(|l| l.resource_pool())
        .unwrap_or_else(|| global_pool().clone())
}

pub fn global_pool() -> &'static ResourcePool {
    static POOL: LazyLock<ResourcePool> = LazyLock::new(|| {
        ResourcePool::new(Resources {
            bytes: 1536 * MIB,
            metadata: 512 * MIB,
            readers: 32,
            handles: 512,
            workers: 4,
            catalogs: 1,
        })
    });
    &POOL
}

#[cfg(test)]
#[path = "../tests/unit/resource_budget.rs"]
mod tests;
