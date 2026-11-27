//! Shared admission and worker policy for evidence imports.
//!
//! CPU admission and bounded worker topology. Resident allocations and reader
//! resources are admitted by source-kernel's process resource pool.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

pub use crate::import_group_gate::{
    global_import_group_gate, ImportGroupGateError, ImportGroupLease,
};

const CLUSTER_MEMBER_CAP: usize = 2;
const CLUSTER_MEMBER_WORKER_CAP: usize = 2;
const ADMISSION_WAIT: Duration = Duration::from_millis(100);
const ADMISSION_WAIT_TIMEOUT: Duration = Duration::from_secs(300);

/// Work topology used to derive a bounded import policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportWorkload {
    SingleSource,
    LinuxEvidenceSet { member_count: usize },
}

/// Resource policy shared by ordinary and evidence-set imports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportSchedulingPolicy {
    pub cpu_budget: usize,
    pub import_workers: usize,
    pub analysis_workers: usize,
    pub source_concurrency: usize,
    pub memory_reservation_mb: u64,
}

impl ImportSchedulingPolicy {
    pub fn for_workload(
        workload: ImportWorkload,
        max_import_workers: Option<usize>,
        max_analysis_workers: Option<usize>,
    ) -> Self {
        let cpu_budget = default_cpu_budget();
        let requested_import = resolve_requested(max_import_workers, cpu_budget);
        let requested_analysis = resolve_requested(max_analysis_workers, cpu_budget);
        match workload {
            ImportWorkload::SingleSource => Self {
                cpu_budget,
                import_workers: requested_import,
                analysis_workers: requested_analysis,
                source_concurrency: 1,
                memory_reservation_mb: source_kernel::resource_budget::WORKSPACE_BYTES
                    / source_kernel::resource_budget::MIB,
            },
            ImportWorkload::LinuxEvidenceSet { member_count } => Self::for_linux_evidence_set(
                cpu_budget,
                requested_import,
                requested_analysis,
                member_count,
            ),
        }
    }

    pub fn for_linux_evidence_set(
        cpu_budget: usize,
        requested_import_workers: usize,
        requested_analysis_workers: usize,
        member_count: usize,
    ) -> Self {
        let cpu_budget = cpu_budget.max(1);
        let member_worker_cap = cpu_budget.clamp(1, CLUSTER_MEMBER_WORKER_CAP);
        let test_member_worker_cap = test_override("FORENSICS_TEST_MEMBER_WORKER_CAP")
            .unwrap_or(member_worker_cap)
            .clamp(1, member_worker_cap);
        let import_workers = requested_import_workers.clamp(1, test_member_worker_cap);
        let analysis_workers = requested_analysis_workers.clamp(1, test_member_worker_cap);
        let per_member_cpu = import_workers.max(analysis_workers);
        let source_concurrency = member_count
            .min(test_cluster_member_cap())
            .min((cpu_budget / per_member_cpu).max(1))
            .min(
                (source_kernel::resource_budget::global_pool().limits().bytes
                    / source_kernel::resource_budget::WORKSPACE_BYTES)
                    .max(1) as usize,
            )
            .max(1);
        let memory_reservation_mb =
            source_kernel::resource_budget::WORKSPACE_BYTES / source_kernel::resource_budget::MIB;
        Self {
            cpu_budget,
            import_workers,
            analysis_workers,
            source_concurrency,
            memory_reservation_mb,
        }
    }

    pub fn admission_request(self) -> ImportAdmissionRequest {
        ImportAdmissionRequest {
            cpu_weight: self.import_workers.max(self.analysis_workers),
        }
    }

    pub fn source_worker_count(self, queued_sources: usize) -> usize {
        if queued_sources == 0 {
            0
        } else {
            queued_sources.min(self.source_concurrency.max(1))
        }
    }
}

/// Bounded resource request for one active source import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportAdmissionRequest {
    pub cpu_weight: usize,
}

/// Snapshot useful for progress reporting and deterministic tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportAdmissionSnapshot {
    pub cpu_capacity: usize,
    pub cpu_in_use: usize,
    pub active_sources: usize,
    pub peak_active_sources: usize,
    pub peak_cpu_in_use: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportAdmissionError {
    Cancelled,
    TimedOut,
}

impl std::fmt::Display for ImportAdmissionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => {
                formatter.write_str("Import cancelled while waiting for scheduler admission")
            }
            Self::TimedOut => formatter.write_str("Import scheduler admission timed out"),
        }
    }
}

impl std::error::Error for ImportAdmissionError {}

struct AdmissionState {
    cpu_in_use: usize,
    active_sources: usize,
    peak_active_sources: usize,
    peak_cpu_in_use: usize,
}

struct AdmissionInner {
    state: Mutex<AdmissionState>,
    changed: Condvar,
    cpu_capacity: usize,
    /// Lock-free live permit count for external observers. Incremented on
    /// successful admission and decremented on [`ImportPermit`] Drop. It sits
    /// outside the `state` mutex on purpose: the post-import scheduler probes
    /// it before claiming new work and must never block on the
    /// Condvar-protected bookkeeping importers wait on.
    active_permits: AtomicU64,
}

/// Process-local weighted admission controller.
#[derive(Clone)]
pub struct ImportAdmission {
    inner: Arc<AdmissionInner>,
}

impl ImportAdmission {
    pub fn new(cpu_capacity: usize) -> Self {
        Self {
            inner: Arc::new(AdmissionInner {
                state: Mutex::new(AdmissionState {
                    cpu_in_use: 0,
                    active_sources: 0,
                    peak_active_sources: 0,
                    peak_cpu_in_use: 0,
                }),
                changed: Condvar::new(),
                cpu_capacity: cpu_capacity.max(1),
                active_permits: AtomicU64::new(0),
            }),
        }
    }

    pub fn acquire(
        &self,
        request: ImportAdmissionRequest,
        cancel_token: &AtomicBool,
    ) -> Result<ImportPermit, ImportAdmissionError> {
        self.acquire_with_timeout(request, cancel_token, ADMISSION_WAIT_TIMEOUT)
    }

    pub fn acquire_with_timeout(
        &self,
        request: ImportAdmissionRequest,
        cancel_token: &AtomicBool,
        timeout: Duration,
    ) -> Result<ImportPermit, ImportAdmissionError> {
        let started = std::time::Instant::now();
        let request = self.normalize_request(request);
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        loop {
            if cancel_token.load(Ordering::Acquire) {
                return Err(ImportAdmissionError::Cancelled);
            }
            if self.can_admit(&state, request) {
                state.cpu_in_use += request.cpu_weight;
                state.active_sources += 1;
                state.peak_active_sources = state.peak_active_sources.max(state.active_sources);
                state.peak_cpu_in_use = state.peak_cpu_in_use.max(state.cpu_in_use);
                self.inner.active_permits.fetch_add(1, Ordering::AcqRel);
                return Ok(ImportPermit {
                    inner: Arc::clone(&self.inner),
                    request,
                });
            }
            if started.elapsed() >= timeout {
                return Err(ImportAdmissionError::TimedOut);
            }
            let wait = ADMISSION_WAIT.min(timeout.saturating_sub(started.elapsed()));
            state = self
                .inner
                .changed
                .wait_timeout(state, wait)
                .unwrap_or_else(|error| error.into_inner())
                .0;
        }
    }

    /// Read-only count of currently held import permits (live imports).
    /// Lock-free relaxed-style probe; callers must not rely on it for
    /// admission decisions themselves — the weighted controller in
    /// `acquire_with_timeout` remains the only admission authority.
    pub fn active_weight(&self) -> u64 {
        self.inner.active_permits.load(Ordering::Acquire)
    }

    pub fn snapshot(&self) -> ImportAdmissionSnapshot {
        let state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        ImportAdmissionSnapshot {
            cpu_capacity: self.inner.cpu_capacity,
            cpu_in_use: state.cpu_in_use,
            active_sources: state.active_sources,
            peak_active_sources: state.peak_active_sources,
            peak_cpu_in_use: state.peak_cpu_in_use,
        }
    }

    fn normalize_request(&self, request: ImportAdmissionRequest) -> ImportAdmissionRequest {
        ImportAdmissionRequest {
            cpu_weight: request.cpu_weight.max(1).min(self.inner.cpu_capacity),
        }
    }

    fn can_admit(&self, state: &AdmissionState, request: ImportAdmissionRequest) -> bool {
        state.cpu_in_use + request.cpu_weight <= self.inner.cpu_capacity
    }
}

/// RAII lease returned by [`ImportAdmission::acquire`].
pub struct ImportPermit {
    inner: Arc<AdmissionInner>,
    request: ImportAdmissionRequest,
}

impl Drop for ImportPermit {
    fn drop(&mut self) {
        // Decrement the lock-free probe before touching the Condvar mutex so a
        // waiting observer never blocks on bookkeeping; the mutex below only
        // updates the tracked snapshot and wakes importers.
        self.inner.active_permits.fetch_sub(1, Ordering::AcqRel);
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.cpu_in_use = state.cpu_in_use.saturating_sub(self.request.cpu_weight);
        state.active_sources = state.active_sources.saturating_sub(1);
        self.inner.changed.notify_all();
    }
}

pub fn global_import_admission() -> &'static ImportAdmission {
    static ADMISSION: OnceLock<ImportAdmission> = OnceLock::new();
    ADMISSION.get_or_init(|| ImportAdmission::new(default_cpu_budget()))
}

pub use source_kernel::resource_budget::{
    default_cpu_budget, resolve_analysis_worker_count, resolve_analysis_worker_count_for_memory,
};

fn test_cluster_member_cap() -> usize {
    test_override("FORENSICS_TEST_CLUSTER_MEMBER_CAP")
        .unwrap_or(CLUSTER_MEMBER_CAP)
        .max(1)
}

fn test_override(name: &str) -> Option<usize> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
}

pub fn resolve_import_worker_count(max_import_workers: Option<usize>) -> usize {
    resolve_requested(max_import_workers, default_cpu_budget())
}

fn resolve_requested(requested: Option<usize>, capacity: usize) -> usize {
    requested
        .filter(|count| *count > 0)
        .unwrap_or(capacity.min(2))
        .min(capacity)
        .max(1)
}
