use super::ExtractionOutcome;
use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};

mod amcache;
mod appcompat;
mod context;
mod dispatch;
mod extractors;
mod ntuser;
mod sam;
mod security;
mod shared;
mod software;
mod system;
mod txlog;
mod usrclass;
mod warnings;

use context::RegistryExtractionContext;
use shared::hive_meta_artifact;
use warnings::govern_registry_warnings;

pub fn extract_registry_candidate(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    boot_key: Option<[u8; 16]>,
    txlog1: Option<&[u8]>,
    txlog2: Option<&[u8]>,
) -> ExtractionOutcome {
    let mut outcome = ExtractionOutcome::default();
    let mut raw_warnings = Vec::new();
    if !bytes.starts_with(b"regf") {
        raw_warnings.push(format!("{} is not a regf registry hive", candidate.path));
        outcome.warnings = govern_registry_warnings(&candidate.path, raw_warnings);
        return outcome;
    }

    let normalized = normalize_evidence_path(&candidate.path);
    let txlog_merged = txlog::merge_status(&candidate.path, txlog1, txlog2, &mut raw_warnings);
    let deleted_keys_found = txlog::count_deleted_cells(&candidate.path, bytes, &mut raw_warnings);

    dispatch::extract(
        &normalized,
        RegistryExtractionContext {
            candidate,
            bytes,
            boot_key,
            txlog1,
            txlog2,
            outcome: &mut outcome,
            warnings: &mut raw_warnings,
        },
    );

    outcome.artifacts.push(hive_meta_artifact(
        candidate,
        txlog_merged,
        deleted_keys_found,
    ));
    raw_warnings.append(&mut outcome.warnings);
    outcome.warnings = govern_registry_warnings(&candidate.path, raw_warnings);
    outcome
}

#[cfg(test)]
#[path = "../../../../tests/unit/analysis_service/extraction/registry/mod.rs"]
mod tests;
