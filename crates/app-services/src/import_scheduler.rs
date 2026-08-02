//! Shared admission and worker policy for evidence imports.
//!
//! The scheduler deliberately owns resource policy only.  Import phases keep
//! ownership of evidence reads, staging, and persistence; this module prevents
//! those phases from multiplying workers across ordinary sources and cluster
//! members.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

const DEFAULT_CPU_CAP: usize = 6;
const CLUSTER_MEMBER_CAP: usize = 2;
const CLUSTER_MEMBER_WORKER_CAP: usize = 3;
const MEMORY_CAPACITY_MB: u64 = 4096;
const SINGLE_SOURCE_MEMORY_RESERVATION_MB: u64 = 4096;
const ANALYSIS_WORKER_MEMORY_RESERVATION_MB: u64 = 512;
const ADMISSION_WAIT: Duration = Duration::from_millis(100);

/// Work topology used to derive a bounded import policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportWorkload {
    SingleSource,
    LinuxCluster { member_count: usize },
}

/// Resource policy shared by ordinary and cluster imports.
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
                memory_reservation_mb: SINGLE_SOURCE_MEMORY_RESERVATION_MB,
            },
            ImportWorkload::LinuxCluster { member_count } => Self::for_linux_cluster(
                cpu_budget,
                requested_import,
                requested_analysis,
                member_count,
            ),
        }
    }

    pub fn for_linux_cluster(
        cpu_budget: usize,
        requested_import_workers: usize,
        requested_analysis_workers: usize,
        member_count: usize,
    ) -> Self {
        let cpu_budget = cpu_budget.max(1);
        // A cluster member is an independent evidence reader.  Cap its local
        // worker budget at three so two members can make progress without
        // multiplying the ordinary six-worker source budget.
        let member_worker_cap = cpu_budget.clamp(1, CLUSTER_MEMBER_WORKER_CAP);
        let import_workers = requested_import_workers.clamp(1, member_worker_cap);
        let analysis_workers = requested_analysis_workers.clamp(1, member_worker_cap);
        let per_member_cpu = import_workers.max(analysis_workers);
        let source_concurrency = member_count
            .min(CLUSTER_MEMBER_CAP)
            .min((cpu_budget / per_member_cpu).max(1))
            .max(1);
        let memory_reservation_mb = (MEMORY_CAPACITY_MB / source_concurrency as u64).max(1024);
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
            memory_mb: self.memory_reservation_mb,
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
    pub memory_mb: u64,
}

/// Snapshot useful for progress reporting and deterministic tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportAdmissionSnapshot {
    pub cpu_capacity: usize,
    pub cpu_in_use: usize,
    pub memory_capacity_mb: u64,
    pub memory_in_use_mb: u64,
    pub active_sources: usize,
    pub peak_active_sources: usize,
    pub peak_cpu_in_use: usize,
    pub peak_memory_in_use_mb: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportAdmissionError {
    Cancelled,
}

impl std::fmt::Display for ImportAdmissionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => {
                formatter.write_str("Import cancelled while waiting for scheduler admission")
            }
        }
    }
}

impl std::error::Error for ImportAdmissionError {}

struct AdmissionState {
    cpu_in_use: usize,
    memory_in_use_mb: u64,
    active_sources: usize,
    peak_active_sources: usize,
    peak_cpu_in_use: usize,
    peak_memory_in_use_mb: u64,
}

struct AdmissionInner {
    state: Mutex<AdmissionState>,
    changed: Condvar,
    cpu_capacity: usize,
    memory_capacity_mb: u64,
}

/// Process-local weighted admission controller.
#[derive(Clone)]
pub struct ImportAdmission {
    inner: Arc<AdmissionInner>,
}

impl ImportAdmission {
    pub fn new(cpu_capacity: usize, memory_capacity_mb: u64) -> Self {
        Self {
            inner: Arc::new(AdmissionInner {
                state: Mutex::new(AdmissionState {
                    cpu_in_use: 0,
                    memory_in_use_mb: 0,
                    active_sources: 0,
                    peak_active_sources: 0,
                    peak_cpu_in_use: 0,
                    peak_memory_in_use_mb: 0,
                }),
                changed: Condvar::new(),
                cpu_capacity: cpu_capacity.max(1),
                memory_capacity_mb: memory_capacity_mb.max(1),
            }),
        }
    }

    pub fn acquire(
        &self,
        request: ImportAdmissionRequest,
        cancel_token: &AtomicBool,
    ) -> Result<ImportPermit, ImportAdmissionError> {
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
                state.memory_in_use_mb += request.memory_mb;
                state.active_sources += 1;
                state.peak_active_sources = state.peak_active_sources.max(state.active_sources);
                state.peak_cpu_in_use = state.peak_cpu_in_use.max(state.cpu_in_use);
                state.peak_memory_in_use_mb =
                    state.peak_memory_in_use_mb.max(state.memory_in_use_mb);
                return Ok(ImportPermit {
                    inner: Arc::clone(&self.inner),
                    request,
                });
            }
            state = self
                .inner
                .changed
                .wait_timeout(state, ADMISSION_WAIT)
                .unwrap_or_else(|error| error.into_inner())
                .0;
        }
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
            memory_capacity_mb: self.inner.memory_capacity_mb,
            memory_in_use_mb: state.memory_in_use_mb,
            active_sources: state.active_sources,
            peak_active_sources: state.peak_active_sources,
            peak_cpu_in_use: state.peak_cpu_in_use,
            peak_memory_in_use_mb: state.peak_memory_in_use_mb,
        }
    }

    fn normalize_request(&self, request: ImportAdmissionRequest) -> ImportAdmissionRequest {
        ImportAdmissionRequest {
            cpu_weight: request.cpu_weight.max(1).min(self.inner.cpu_capacity),
            memory_mb: request.memory_mb.max(1).min(self.inner.memory_capacity_mb),
        }
    }

    fn can_admit(&self, state: &AdmissionState, request: ImportAdmissionRequest) -> bool {
        let cpu_available = state.cpu_in_use + request.cpu_weight <= self.inner.cpu_capacity;
        let memory_available =
            state.memory_in_use_mb + request.memory_mb <= self.inner.memory_capacity_mb;
        if !cpu_available || !memory_available {
            return false;
        }
        // RSS is a safety signal, not the source of truth for reservations. If
        // one source is already active, wait for it to drain below the soft
        // capacity before admitting another source. Always admit the first
        // source so a high baseline RSS cannot deadlock an import forever.
        let rss_mb = crate::runtime_resources::current_rss_mb();
        state.cpu_in_use == 0 || rss_mb == 0 || rss_mb < self.inner.memory_capacity_mb
    }
}

/// RAII lease returned by [`ImportAdmission::acquire`].
pub struct ImportPermit {
    inner: Arc<AdmissionInner>,
    request: ImportAdmissionRequest,
}

impl Drop for ImportPermit {
    fn drop(&mut self) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.cpu_in_use = state.cpu_in_use.saturating_sub(self.request.cpu_weight);
        state.memory_in_use_mb = state
            .memory_in_use_mb
            .saturating_sub(self.request.memory_mb);
        state.active_sources = state.active_sources.saturating_sub(1);
        self.inner.changed.notify_all();
    }
}

pub fn global_import_admission() -> &'static ImportAdmission {
    static ADMISSION: OnceLock<ImportAdmission> = OnceLock::new();
    ADMISSION.get_or_init(|| ImportAdmission::new(default_cpu_budget(), MEMORY_CAPACITY_MB))
}

pub fn default_cpu_budget() -> usize {
    std::thread::available_parallelism()
        .map(|count| count.get().min(DEFAULT_CPU_CAP))
        .unwrap_or(DEFAULT_CPU_CAP)
        .max(1)
}

pub fn resolve_import_worker_count(max_import_workers: Option<usize>) -> usize {
    resolve_requested(max_import_workers, default_cpu_budget())
}

pub fn resolve_analysis_worker_count(max_analysis_workers: Option<usize>) -> usize {
    resolve_requested(max_analysis_workers, default_cpu_budget())
}

/// Resolve analysis workers against both CPU capacity and the process RSS
/// soft limit. A worker is given a conservative memory reservation so a small
/// CPU increase cannot turn into an unbounded peak when several content files
/// are processed at once. A zero RSS/limit keeps deterministic callers from
/// being throttled when the platform cannot provide memory telemetry.
pub fn resolve_analysis_worker_count_for_memory(
    max_analysis_workers: Option<usize>,
    rss_mb: u64,
    memory_soft_limit_mb: u64,
) -> usize {
    let cpu_bound = resolve_analysis_worker_count(max_analysis_workers);
    if rss_mb == 0 || memory_soft_limit_mb == 0 {
        return cpu_bound;
    }
    let available_mb = memory_soft_limit_mb.saturating_sub(rss_mb);
    let memory_bound = (available_mb / ANALYSIS_WORKER_MEMORY_RESERVATION_MB).max(1);
    let memory_bound = usize::try_from(memory_bound).unwrap_or(usize::MAX);
    cpu_bound.min(memory_bound).max(1)
}

fn resolve_requested(requested: Option<usize>, capacity: usize) -> usize {
    requested
        .filter(|count| *count > 0)
        .unwrap_or(capacity)
        .min(capacity)
        .max(1)
}
