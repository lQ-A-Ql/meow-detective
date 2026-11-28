use std::{
    collections::BTreeMap,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

use domain::structure::{CapabilityIntegrity, StructureRequest, TargetCoverage, TargetResult};
use rusqlite::Connection;
use source_kernel::source_runtime::verification::VerificationBatch;

use super::{
    frontier::CapabilityEntry, ActivityDefinition, ActivityProof, RuntimeError, StructureOperation,
};

pub struct ExecutionContext<'a> {
    pub connection: &'a Connection,
    pub case_root: &'a Path,
    pub request: &'a StructureRequest,
    pub attempt_id: &'a str,
    pub owner_id: &'a str,
    pub batch: &'a VerificationBatch,
    pub input_proofs: &'a BTreeMap<String, Arc<ActivityProof>>,
    pub cancel: &'a AtomicBool,
}

pub struct ActivityOutput {
    pub proof: Arc<ActivityProof>,
    pub capabilities: Vec<CapabilityEntry>,
    pub target_results: Vec<TargetResult>,
    pub analysis: Option<super::LayerAnalysisResult>,
}

pub(super) fn execute_in_worker(
    operation: &dyn StructureOperation,
    activity: &ActivityDefinition,
    context: &ExecutionContext<'_>,
) -> Result<ActivityOutput, RuntimeError> {
    let root = context.case_root;
    let request = context.request;
    let attempt_id = context.attempt_id;
    let owner_id = context.owner_id;
    let batch = context.batch;
    let input_proofs = context.input_proofs;
    let cancel = context.cancel;
    std::thread::scope(|workers| {
        let worker = workers.spawn(move || {
            let connection =
                persistence_sqlite::open_existing_case_read_only(&root.join("app.db"))?;
            operation.execute(
                activity,
                &ExecutionContext {
                    connection: &connection,
                    case_root: root,
                    request,
                    attempt_id,
                    owner_id,
                    batch,
                    input_proofs,
                    cancel,
                },
            )
        });
        match worker.join() {
            Ok(result) => result,
            Err(reason) => std::panic::resume_unwind(reason),
        }
    })
}

impl ActivityOutput {
    pub fn validate(&self, activity: &ActivityDefinition) -> Result<(), RuntimeError> {
        self.proof.validate()?;
        let receipt_id = self.proof.id();
        if let ActivityProof::Snapshot(proof) = self.proof.as_ref() {
            if self.analysis.is_none() {
                return Err(RuntimeError::InvalidOutput);
            }
            if proof.receipt().target_id
                != activity
                    .target
                    .identity()
                    .map_err(|_| RuntimeError::InvalidOutput)?
                || proof.receipt().input_capability_ids != activity.work.input_capability_versions
            {
                return Err(RuntimeError::InvalidOutput);
            }
        }
        if let Some(analysis) = &self.analysis {
            let ActivityProof::Snapshot(proof) = self.proof.as_ref() else {
                return Err(RuntimeError::InvalidOutput);
            };
            let bytes =
                serde_json::to_vec(&analysis.details).map_err(|_| RuntimeError::InvalidOutput)?;
            if analysis.receipt != *proof.receipt()
                || analysis.source_revision != proof.receipt().source_revision
                || infrastructure::hashing::sha256_bytes(&bytes) != proof.receipt().output_sha256
                || analysis.profile_id != activity.work.adapter_id
                || analysis.storage.receipt_id != proof.receipt().id
                || analysis.storage.source_id != analysis.source_revision.source_id
                || analysis.storage.output_sha256 != proof.receipt().output_sha256
                || analysis.storage.generation_name
                    != format!("analysis-results/{}", proof.receipt().id)
            {
                return Err(RuntimeError::InvalidOutput);
            }
        }
        let target_id = activity
            .target
            .identity()
            .map_err(|_| RuntimeError::InvalidOutput)?;
        for entry in &self.capabilities {
            entry.validate()?;
            if entry.scope_id != activity.scope_id
                || entry.capability.target_id != target_id
                || entry.capability.view_id != activity.target.view_id()
                || !same_request(&activity.requested_coverage, &entry.capability.coverage)
            {
                return Err(RuntimeError::InvalidOutput);
            }
            if let CapabilityIntegrity::ContentVerified { content_sha256, .. } =
                &entry.capability.integrity
            {
                if content_sha256 != self.proof.output_sha256()
                    || self.proof.read_receipt().is_none()
                {
                    return Err(RuntimeError::InvalidOutput);
                }
            }
            validate_byte_proof(&entry.capability.coverage, &self.proof)?;
        }
        for result in &self.target_results {
            result.validate().map_err(|_| RuntimeError::InvalidOutput)?;
            if result.target_id != target_id
                || result.evidence_ids != std::collections::BTreeSet::from([receipt_id.to_owned()])
                || result.required_capabilities != activity.required_capabilities
                || !same_request(&activity.requested_coverage, &result.coverage)
            {
                return Err(RuntimeError::InvalidOutput);
            }
            validate_byte_proof(&result.coverage, &self.proof)?;
            if matches!(self.proof.as_ref(), ActivityProof::Snapshot(_)) {
                validate_summary_page(&result.coverage, self.proof.output_sha256())?;
            }
        }
        if self.capabilities.is_empty() && self.target_results.is_empty() {
            return Err(RuntimeError::InvalidOutput);
        }
        Ok(())
    }
}

fn validate_summary_page(
    coverage: &TargetCoverage,
    output_sha256: &str,
) -> Result<(), RuntimeError> {
    let TargetCoverage::Namespace(namespace) = coverage else {
        return Err(RuntimeError::InvalidOutput);
    };
    let [page] = namespace.pages.as_slice() else {
        return Err(RuntimeError::InvalidOutput);
    };
    if page.input_cursor.is_some()
        || page.first_ordinal != 0
        || page.object_count != 1
        || !page.verified
        || page.page_sha256 != output_sha256
        || !matches!(&page.end, domain::structure::NamespacePageEnd::Eof { witness_sha256 } if witness_sha256 == output_sha256)
    {
        return Err(RuntimeError::InvalidOutput);
    }
    Ok(())
}

fn same_request(expected: &TargetCoverage, actual: &TargetCoverage) -> bool {
    match (expected, actual) {
        (TargetCoverage::Bytes(left), TargetCoverage::Bytes(right)) => {
            left.target_size == right.target_size && left.requested == right.requested
        }
        (TargetCoverage::Namespace(left), TargetCoverage::Namespace(right)) => {
            left.context == right.context
                && left.root_id == right.root_id
                && left.query_sha256 == right.query_sha256
                && left.ordering_id == right.ordering_id
        }
        (TargetCoverage::Keys(left), TargetCoverage::Keys(right)) => {
            left.context == right.context
                && left.requested == right.requested
                && left.ordering == right.ordering
        }
        (TargetCoverage::Events(left), TargetCoverage::Events(right)) => {
            left.context == right.context
                && left.requested == right.requested
                && left.sequence_domain == right.sequence_domain
                && left.native_ordering_id == right.native_ordering_id
                && left.ordering == right.ordering
                && left.event_clock == right.event_clock
                && left.observation_clock == right.observation_clock
                && left.receipt_clock == right.receipt_clock
        }
        (TargetCoverage::Runtime(left), TargetCoverage::Runtime(right)) => {
            left.context == right.context
                && left.snapshot_id == right.snapshot_id
                && left.requested == right.requested
        }
        _ => false,
    }
}

fn validate_byte_proof(
    coverage: &TargetCoverage,
    proof: &ActivityProof,
) -> Result<(), RuntimeError> {
    if let TargetCoverage::Bytes(bytes) = coverage {
        let receipt = proof.read_receipt().ok_or(RuntimeError::InvalidOutput)?;
        if bytes.target_size != receipt.ranges.target_size()
            || !bytes.native_holes.is_empty()
            || bytes.verified.iter().any(|verified| {
                !receipt
                    .ranges
                    .ranges()
                    .iter()
                    .any(|range| range.start() <= verified.start() && range.end() >= verified.end())
            })
        {
            return Err(RuntimeError::InvalidOutput);
        }
    }
    Ok(())
}
