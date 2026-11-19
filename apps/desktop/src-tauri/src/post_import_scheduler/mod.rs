//! Hardened post-import capability scheduler (L2).
//!
//! After a catalog-only import job completes, `timeline` and `artifacts` rows
//! marked `queued`/`pending` are executed on a bounded worker pool. L2 adds:
//!
//! * priority admission: P0 `user_retry` (manual retry) ahead of P1
//!   `post_import_auto` (import completion / case-open replay);
//! * per-source serialization: the same source runs one post-processing task
//!   at a time, different sources may run in parallel;
//! * CAS claim: a worker only executes after `SourceCapabilityRepo::try_claim`
//!   flips `queued`/`pending` to `running`;
//! * restart reconciliation, cancel tokens, and bounded P0 retry slots;
//! * idempotent pre-checks (Complete projection / task-confirmed ready) before
//!   re-running work.
//!
//! The pool size mirrors `resolve_analysis_worker_count(None)`: CPU-bound
//! analysis workers default to a cap of 2, so the pool never grows unbounded.
//! State and admission live in `state.rs`, execution in `worker.rs`, teardown
//! in `cancel.rs`; this file stays the API surface.

mod cancel;
mod dispatch;
mod enqueue;
mod events;
mod state;
mod worker;

pub(crate) use cancel::{cancel_case, cancel_source};
pub(crate) use state::Admission;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum QueuedCapability {
    Timeline,
    Artifacts,
}

impl QueuedCapability {
    pub(super) fn wire(self) -> &'static str {
        match self {
            Self::Timeline => "timeline",
            Self::Artifacts => "artifacts",
        }
    }

    pub(crate) fn parse(wire: &str) -> Option<Self> {
        match wire {
            "timeline" => Some(Self::Timeline),
            "artifacts" => Some(Self::Artifacts),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Priority {
    /// P0: manual user retry (`retry_source_capability`).
    UserRetry,
    /// P1: automatic post-import work (completion hook / case-open replay).
    Auto,
}

pub(super) struct QueuedTask {
    pub(super) app_state: crate::state::AppState,
    pub(super) app: Option<tauri::AppHandle>,
    pub(super) case_id: String,
    pub(super) case_root: std::path::PathBuf,
    pub(super) source_id: String,
    pub(super) capability: QueuedCapability,
    pub(super) priority: Priority,
    pub(super) revision: i64,
}

impl QueuedTask {
    /// Dedup key: `(case_id, source_id, capability, revision)`. A revision bump
    /// (e.g. after a state transition) breaks dedup and admits the newer
    /// generation instead of reusing a stale slot.
    fn key(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.case_id,
            self.source_id,
            self.capability.wire(),
            self.revision
        )
    }

    /// Family prefix locating the live token / queued entry without revision.
    fn family_key(&self) -> String {
        format!(
            "{}|{}|{}",
            self.case_id,
            self.source_id,
            self.capability.wire()
        )
    }

    /// Per-source serialization key within this case.
    fn source_key(&self) -> String {
        format!("{}|{}", self.case_id, self.source_id)
    }
}

pub(crate) use enqueue::{enqueue_case_pending, enqueue_for_source, retry_admission};
