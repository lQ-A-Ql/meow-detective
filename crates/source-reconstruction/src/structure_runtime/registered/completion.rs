use std::collections::BTreeSet;

use domain::structure::{
    CapabilityKind, StructureStage, TargetCoverage, TargetResult, TargetSelector, TargetStatus,
};

use super::super::{
    proof_state, ActivityDefinition, ActivityOutput, BindingContext, BindingDecision,
    ExecutableDescriptor, ExecutionContext, RuntimeError, StructureOperation,
};
use super::{
    definition::{definition, requested_coverage},
    ADAPTER, TARGET, VERSION,
};

pub(super) struct CompleteBlockOperation;

impl StructureOperation for CompleteBlockOperation {
    fn descriptor(&self) -> ExecutableDescriptor {
        ExecutableDescriptor {
            adapter_id: ADAPTER.into(),
            adapter_version: VERSION.into(),
            operation_id: TARGET.into(),
            stage: StructureStage::ReadVerification,
            input_kind: CapabilityKind::BlockRange,
            input_role: "verified-block".into(),
        }
    }

    fn bind(&self, context: &BindingContext<'_>) -> Result<BindingDecision, RuntimeError> {
        let TargetCoverage::Bytes(coverage) = &context.changed.capability.coverage else {
            return Err(RuntimeError::InvalidDefinition);
        };
        if coverage.requested.len() != 1 {
            return Err(RuntimeError::InvalidDefinition);
        };
        let target = TargetSelector::Block {
            view_id: context.changed.capability.view_id.clone(),
            range: coverage.requested[0],
        };
        Ok(BindingDecision::Bound(vec![definition(
            self.descriptor(),
            context.changed,
            target,
            requested_coverage(coverage.target_size, coverage.requested[0]),
            serde_json::Value::Null,
            &context.request.goals,
        )?]))
    }

    fn execute(
        &self,
        activity: &ActivityDefinition,
        context: &ExecutionContext<'_>,
    ) -> Result<ActivityOutput, RuntimeError> {
        let id = activity
            .work
            .input_capability_versions
            .first()
            .ok_or(RuntimeError::InvalidDefinition)?;
        let proof = context
            .input_proofs
            .get(id)
            .ok_or(RuntimeError::InvalidOutput)?
            .clone();
        proof.validate().map_err(proof_state::classify)?;
        let TargetCoverage::Bytes(mut coverage) = activity.requested_coverage.clone() else {
            return Err(RuntimeError::InvalidDefinition);
        };
        coverage.verified = std::mem::take(&mut coverage.unknown);
        let result = TargetResult {
            target_id: activity
                .target
                .identity()
                .map_err(|_| RuntimeError::InvalidDefinition)?,
            status: TargetStatus::Verified,
            required_capabilities: activity.required_capabilities.clone(),
            coverage: TargetCoverage::Bytes(coverage),
            evidence_ids: BTreeSet::from([proof.id().to_owned()]),
            error: None,
        };
        Ok(ActivityOutput {
            proof,
            capabilities: vec![],
            target_results: vec![result],
            analysis: None,
        })
    }
}
