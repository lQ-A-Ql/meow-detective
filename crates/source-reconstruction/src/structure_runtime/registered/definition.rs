use std::collections::BTreeSet;

use domain::structure::{
    canonical_identity, ByteCoverage, ByteRange, CapabilityKind, OrderingRequirement,
    StructureGoal, TargetCoverage, TargetSelector, WorkIdentity,
};

use super::super::{
    activity::identity_digest, frontier::CapabilityEntry, ActivityDefinition, ExecutableDescriptor,
    RuntimeError,
};

pub(super) fn definition(
    descriptor: ExecutableDescriptor,
    entry: &CapabilityEntry,
    target: TargetSelector,
    requested_coverage: TargetCoverage,
    parameters: serde_json::Value,
    goals: &BTreeSet<StructureGoal>,
) -> Result<ActivityDefinition, RuntimeError> {
    Ok(ActivityDefinition {
        work: WorkIdentity {
            operation_id: descriptor.operation_id,
            adapter_id: descriptor.adapter_id,
            adapter_version: descriptor.adapter_version,
            goal_id: canonical_identity("structure-goals-v1", goals)?,
            input_capability_versions: BTreeSet::from([entry.capability.id.clone()]),
            selector_sha256: identity_digest("target-selector-v1", &target)?,
            version_witness_sha256: identity_digest("capability-version-v1", &entry.capability)?,
            scope_witness_sha256: identity_digest("binding-scope-v1", &entry.scope_id)?,
            ordering: OrderingRequirement::Unordered,
            policy_version: "structure-runtime-v1".into(),
        },
        stage: descriptor.stage,
        target,
        scope_id: entry.scope_id.clone(),
        required_capabilities: BTreeSet::from([CapabilityKind::BlockRange]),
        requested_coverage,
        dependencies: BTreeSet::new(),
        parameters,
    })
}

pub(super) fn requested_coverage(target_size: u64, range: ByteRange) -> TargetCoverage {
    TargetCoverage::Bytes(ByteCoverage {
        target_size,
        requested: vec![range],
        verified: vec![],
        native_holes: vec![],
        unknown: vec![range],
        unreadable: vec![],
    })
}
