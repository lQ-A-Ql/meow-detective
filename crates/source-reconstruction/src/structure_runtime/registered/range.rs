use std::collections::BTreeSet;

use domain::structure::{
    canonical_identity, ByteCoverage, ByteRange, ByteRangeSet, CapabilityIntegrity, CapabilityKind,
    CapabilityRef, ReadDemand, ReadIntent, ReadVerification, StructureGoal, StructureStage,
    TargetCoverage, TargetFailure, TargetSelector, VersionSelection,
};
use evidence_access::structure_access::{
    verify_registered_access, RegisteredAccessRequest, TargetAccessError,
};
use persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo;

use super::super::{
    frontier::CapabilityEntry, ActivityDefinition, ActivityOutput, BindingContext, BindingDecision,
    ExecutableDescriptor, ExecutionContext, RuntimeError, StructureOperation,
};
use super::{
    definition::{definition, requested_coverage},
    RegisteredParameters, ADAPTER, FREEZE, VERSION,
};

pub(super) struct RegisteredRangeOperation;

impl StructureOperation for RegisteredRangeOperation {
    fn descriptor(&self) -> ExecutableDescriptor {
        ExecutableDescriptor {
            adapter_id: ADAPTER.into(),
            adapter_version: VERSION.into(),
            operation_id: FREEZE.into(),
            stage: StructureStage::SourceFreeze,
            input_kind: CapabilityKind::AcquiredBytes,
            input_role: "acquired-source".into(),
        }
    }

    fn bind(&self, context: &BindingContext<'_>) -> Result<BindingDecision, RuntimeError> {
        let entry = context.changed;
        match &context.request.version_selection {
            VersionSelection::Current => {}
            VersionSelection::Exact { view_id } if view_id == &entry.capability.view_id => {}
            _ => return Err(RuntimeError::InvalidDefinition),
        }
        let source = context
            .request
            .source_scope
            .iter()
            .find(|capture| {
                canonical_identity("registered-source-view-v1", *capture)
                    .is_ok_and(|id| id == entry.capability.view_id)
            })
            .ok_or(RuntimeError::InvalidDefinition)?;
        let TargetCoverage::Bytes(coverage) = &entry.capability.coverage else {
            return Err(RuntimeError::InvalidDefinition);
        };
        let mut targets = context.request.targets.iter().filter(|target| {
            matches!(target, TargetSelector::Block { view_id, .. } if view_id == &entry.capability.view_id)
        }).cloned().collect::<Vec<_>>();
        if targets.is_empty() && context.request.goals.contains(&StructureGoal::Inventory) {
            targets.push(TargetSelector::Block {
                view_id: entry.capability.view_id.clone(),
                range: ByteRange::new(0, coverage.target_size.min(4096))
                    .map_err(|_| RuntimeError::InvalidDefinition)?,
            });
        }
        let mut definitions = Vec::new();
        for target in targets {
            let TargetSelector::Block { range, .. } = target else {
                unreachable!()
            };
            range
                .validate(coverage.target_size)
                .map_err(|_| RuntimeError::InvalidDefinition)?;
            let parameters = RegisteredParameters {
                source: source.source.clone(),
                target_size: coverage.target_size,
            };
            definitions.push(definition(
                self.descriptor(),
                entry,
                target,
                requested_coverage(coverage.target_size, range),
                serde_json::to_value(parameters).map_err(|_| RuntimeError::InvalidDefinition)?,
                &context.request.goals,
            )?);
        }
        Ok(BindingDecision::Bound(definitions))
    }

    fn execute(
        &self,
        activity: &ActivityDefinition,
        context: &ExecutionContext<'_>,
    ) -> Result<ActivityOutput, RuntimeError> {
        if !context.request.goals.iter().all(|goal| {
            matches!(
                goal,
                StructureGoal::Inventory | StructureGoal::RangeRead | StructureGoal::DeepVerify
            )
        }) {
            return Err(RuntimeError::Target(TargetFailure::UnsupportedFeature {
                capability: CapabilityKind::BlockRange,
            }));
        }
        let parameters: RegisteredParameters = serde_json::from_value(activity.parameters.clone())
            .map_err(|_| RuntimeError::InvalidDefinition)?;
        let TargetSelector::Block { range, .. } = activity.target else {
            return Err(RuntimeError::InvalidDefinition);
        };
        let (_, source) = SourceRevisionRepo::new(context.connection)
            .current(&parameters.source.case_id, &parameters.source.source_id)?;
        let capture = context
            .request
            .source_scope
            .iter()
            .find(|capture| {
                capture.source == parameters.source
                    && capture.acquisition_id == source.acquisition_id
            })
            .ok_or(RuntimeError::InvalidDefinition)?;
        if activity.target.view_id() != canonical_identity("registered-source-view-v1", capture)?
            || parameters.target_size != source.logical_size
        {
            return Err(RuntimeError::InvalidDefinition);
        }
        let demand = ReadDemand::new(
            parameters.source.clone(),
            format!(
                "source-revision:{}:{}",
                parameters.source.source_id, parameters.source.revision
            ),
            ReadIntent::ContentRead,
            ByteRangeSet::new(parameters.target_size, [range])
                .map_err(|_| RuntimeError::InvalidDefinition)?,
            context.request.resource_policy.priority,
            context.request.resource_policy.deadline_unix_ms,
            true,
            vec![activity.key()?],
            ReadVerification::ContentDigest,
        )
        .map_err(|_| RuntimeError::InvalidDefinition)?
        .with_attempt(domain::structure::StructureAttemptRef {
            attempt_id: context.attempt_id.into(),
            owner: context.owner_id.into(),
        })
        .map_err(|_| RuntimeError::InvalidDefinition)?;
        let expected = (range.start() == 0 && range.length() == parameters.target_size)
            .then_some(source.logical_sha256.as_deref())
            .flatten();
        let proof = verify_registered_access(
            context.connection,
            context.case_root,
            RegisteredAccessRequest {
                demand,
                file_id: None,
            },
            context.batch,
            VERSION,
            expected,
            false,
            context.cancel,
        )
        .map_err(target_error)?;
        let control = source_kernel::source_db::open_registered_source_db_read_only(
            context.connection,
            context.case_root,
            &domain::DataSourceId(parameters.source.source_id.clone()),
        )?;
        let content_binding = persistence_source::detail::read_physical_content_binding(
            &control,
            &parameters.source,
        )?;
        range_output(
            activity,
            parameters,
            range,
            proof,
            content_binding.manifest_sha256,
        )
    }
}

fn range_output(
    activity: &ActivityDefinition,
    parameters: RegisteredParameters,
    range: ByteRange,
    proof: std::sync::Arc<source_kernel::source_runtime::verification::ValidatedReceipt>,
    manifest_sha256: String,
) -> Result<ActivityOutput, RuntimeError> {
    let receipt = proof.receipt();
    let coverage = TargetCoverage::Bytes(ByteCoverage {
        target_size: parameters.target_size,
        requested: vec![range],
        verified: vec![range],
        native_holes: vec![],
        unknown: vec![],
        unreadable: vec![],
    });
    let capability = CapabilityRef::new(
        activity
            .target
            .identity()
            .map_err(|_| RuntimeError::InvalidDefinition)?,
        activity.target.view_id().into(),
        CapabilityKind::BlockRange,
        parameters.source.revision,
        CapabilityIntegrity::ContentVerified {
            manifest_sha256,
            content_sha256: receipt.output_sha256.clone(),
        },
        coverage,
    )
    .map_err(|_| RuntimeError::InvalidOutput)?;
    Ok(ActivityOutput {
        proof,
        capabilities: vec![CapabilityEntry {
            capability,
            scope_id: activity.scope_id.clone(),
            native_roles: BTreeSet::from(["verified-block".into()]),
        }],
        target_results: vec![],
    })
}

fn target_error(error: TargetAccessError) -> RuntimeError {
    use domain::structure::{CancellationScope, MetadataFailure};
    match error {
        TargetAccessError::Cancelled => RuntimeError::Target(TargetFailure::Cancelled {
            scope: CancellationScope::Target,
        }),
        TargetAccessError::Io(error) => super::super::proof_state::classify(error),
        TargetAccessError::UnsupportedTarget => {
            RuntimeError::Target(TargetFailure::UnsupportedFeature {
                capability: CapabilityKind::BlockRange,
            })
        }
        TargetAccessError::Database(error) => RuntimeError::Database(error),
        _ => RuntimeError::Target(TargetFailure::CorruptMetadata {
            reason: MetadataFailure::InvalidStructure,
        }),
    }
}
