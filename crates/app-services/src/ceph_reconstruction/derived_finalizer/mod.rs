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

pub(super) use catalog::{
    begin_catalog_phase, complete_catalog_phase, fail_catalog_phase, start_catalog_heartbeat,
};
pub(super) use coordinator::finalize_derived_source;
pub(super) use fingerprint::{catalog_phase_is_current, phase_input_fingerprint_for_catalog};
pub(super) use outcome::DerivedFinalizationReport;
pub(super) use phase_runner::{PhaseClaim, ProcessingPhaseAttempt};

#[cfg(test)]
#[path = "../../../tests/unit/ceph_reconstruction/derived_finalizer.rs"]
mod tests;
