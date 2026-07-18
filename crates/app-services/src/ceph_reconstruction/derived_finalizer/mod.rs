use std::path::Path;
use std::sync::{atomic::AtomicBool, Arc};

use domain::{CaseId, DataSourceId};

mod artifacts;
mod catalog;
mod coordinator;
mod fingerprint;
mod graph;
mod outcome;
mod phase_execution;
mod phase_runner;
mod platform;
mod projections;
mod queue;

pub(super) use catalog::{
    begin_catalog_phase, complete_catalog_phase, defer_catalog_phase, fail_catalog_phase,
    refresh_catalog_claim, start_catalog_heartbeat,
};
pub(super) use coordinator::finalize_derived_source;
pub(super) use fingerprint::{catalog_phase_is_current, phase_input_fingerprint_for_catalog};
pub(super) use outcome::DerivedFinalizationReport;
pub(super) use phase_runner::{PhaseClaim, ProcessingPhaseAttempt};
pub(super) use queue::queue_post_catalog_phases;

pub(super) struct DerivedSourceContext<'a> {
    pub(super) case_conn: &'a rusqlite::Connection,
    pub(super) case_root: &'a Path,
    pub(super) case_id: &'a CaseId,
    pub(super) data_source_id: &'a DataSourceId,
    pub(super) cancel_token: &'a Arc<AtomicBool>,
}

#[cfg(test)]
#[path = "../../../tests/unit/ceph_reconstruction/derived_finalizer.rs"]
mod tests;
