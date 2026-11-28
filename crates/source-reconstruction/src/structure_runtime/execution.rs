use std::{
    collections::BTreeMap,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

use domain::structure::{CapabilityIntegrity, StructureRequest, TargetCoverage, TargetResult};
use rusqlite::Connection;
use source_kernel::source_runtime::verification::{ValidatedReceipt, VerificationBatch};

use super::{frontier::CapabilityEntry, ActivityDefinition, RuntimeError, StructureOperation};

pub struct ExecutionContext<'a> {
    pub connection: &'a Connection,
    pub case_root: &'a Path,
    pub request: &'a StructureRequest,
    pub attempt_id: &'a str,
    pub owner_id: &'a str,
    pub batch: &'a VerificationBatch,
    pub input_proofs: &'a BTreeMap<String, Arc<ValidatedReceipt>>,
    pub cancel: &'a AtomicBool,
}

pub struct ActivityOutput {
    pub proof: Arc<ValidatedReceipt>,
    pub capabilities: Vec<CapabilityEntry>,
    pub target_results: Vec<TargetResult>,
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
        let receipt = self.proof.receipt();
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
                if content_sha256 != &receipt.output_sha256 {
                    return Err(RuntimeError::InvalidOutput);
                }
            }
            validate_byte_proof(&entry.capability.coverage, receipt)?;
        }
        for result in &self.target_results {
            result.validate().map_err(|_| RuntimeError::InvalidOutput)?;
            if result.target_id != target_id
                || result.evidence_ids != std::collections::BTreeSet::from([receipt.id.clone()])
                || result.required_capabilities != activity.required_capabilities
                || !same_request(&activity.requested_coverage, &result.coverage)
            {
                return Err(RuntimeError::InvalidOutput);
            }
            validate_byte_proof(&result.coverage, receipt)?;
        }
        if self.capabilities.is_empty() && self.target_results.is_empty() {
            return Err(RuntimeError::InvalidOutput);
        }
        Ok(())
    }
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
    receipt: &domain::structure::VerificationReceipt,
) -> Result<(), RuntimeError> {
    if let TargetCoverage::Bytes(bytes) = coverage {
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
