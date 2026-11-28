mod ceph;
mod guest_linux;
mod kubernetes;
mod planning;

use std::{collections::BTreeSet, sync::Arc};

use domain::structure::{
    NamespacePage, NamespacePageEnd, SourceRevisionRef, StructureAttemptRef, StructureStage,
    TargetCoverage, TargetResult, TargetStatus,
};
use serde::{Deserialize, Serialize};
use source_kernel::source_runtime::snapshot_analysis::SnapshotAnalysisSession;

use super::{
    ActivityDefinition, ActivityOutput, ActivityProof, BindingContext, BindingDecision,
    ExecutableDescriptor, ExecutableRegistry, ExecutionContext, RuntimeError, StructureOperation,
};

pub use planning::{analysis_source_seeds, LayerAnalysisProfile};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayerAnalysisResult {
    pub profile_id: String,
    pub source_revision: SourceRevisionRef,
    pub details: serde_json::Value,
    pub receipt: domain::structure::SnapshotAnalysisReceipt,
    pub storage: persistence_source::analysis_results::AnalysisResultRef,
}

pub fn register_layer_analysis(registry: &mut ExecutableRegistry) -> Result<(), RuntimeError> {
    for profile in [
        LayerAnalysisProfile::CephOsdMetadata,
        LayerAnalysisProfile::CephFsNamespace,
        LayerAnalysisProfile::KubernetesMetadata,
        LayerAnalysisProfile::GuestLinuxArtifacts,
    ] {
        registry.register(Box::new(LayerAnalysisOperation { profile }))?;
    }
    Ok(())
}

struct LayerAnalysisOperation {
    profile: LayerAnalysisProfile,
}

impl StructureOperation for LayerAnalysisOperation {
    fn descriptor(&self) -> ExecutableDescriptor {
        ExecutableDescriptor {
            adapter_id: self.profile.id().into(),
            adapter_version: "layer-analysis-v1".into(),
            operation_id: "analyze-published-native-state".into(),
            stage: StructureStage::InvestigationAnalysis,
            input_kind: self.profile.input_kind(),
            input_role: self.profile.role().into(),
        }
    }

    fn bind(&self, context: &BindingContext<'_>) -> Result<BindingDecision, RuntimeError> {
        planning::bind(self.profile, self.descriptor(), context)
    }

    fn execute(
        &self,
        activity: &ActivityDefinition,
        context: &ExecutionContext<'_>,
    ) -> Result<ActivityOutput, RuntimeError> {
        let parameters: planning::AnalysisParameters =
            serde_json::from_value(activity.parameters.clone())
                .map_err(|_| RuntimeError::InvalidDefinition)?;
        let target_id = activity
            .target
            .identity()
            .map_err(|_| RuntimeError::InvalidDefinition)?;
        let session = SnapshotAnalysisSession::acquire(
            context.case_root,
            &parameters.source,
            target_id.clone(),
            activity.work.input_capability_versions.clone(),
            StructureAttemptRef {
                attempt_id: context.attempt_id.into(),
                owner: context.owner_id.into(),
            },
            self.profile.id().into(),
            "layer-analysis-v1".into(),
        )
        .map_err(super::proof_state::classify)?;
        if planning::generation_identity(context.connection, context.case_root, &parameters.source)?
            != parameters.generation_identity
        {
            return Err(RuntimeError::Target(
                domain::structure::TargetFailure::Conflicted {
                    reason: domain::structure::ConflictReason::InputChanged,
                },
            ));
        }
        let details = match self.profile {
            LayerAnalysisProfile::CephOsdMetadata => {
                ceph::analyze_osd(context, &parameters.source)?
            }
            LayerAnalysisProfile::CephFsNamespace => {
                ceph::analyze_namespace(context, &parameters.source, &parameters.native_id)?
            }
            LayerAnalysisProfile::KubernetesMetadata => {
                kubernetes::analyze(context, &parameters.source)?
            }
            LayerAnalysisProfile::GuestLinuxArtifacts => {
                guest_linux::analyze(context, &parameters.source)?
            }
        };
        validate_output_budget(&details, context.request.resource_policy.page_bytes)?;
        let proof = session
            .seal(&details)
            .map_err(super::proof_state::classify)?;
        let TargetCoverage::Namespace(mut coverage) = activity.requested_coverage.clone() else {
            return Err(RuntimeError::InvalidDefinition);
        };
        coverage.pages.push(NamespacePage {
            input_cursor: None,
            first_ordinal: 0,
            object_count: 1,
            page_sha256: proof.receipt().output_sha256.clone(),
            verified: true,
            end: NamespacePageEnd::Eof {
                witness_sha256: proof.receipt().output_sha256.clone(),
            },
        });
        let result = TargetResult {
            target_id,
            status: TargetStatus::Verified,
            required_capabilities: activity.required_capabilities.clone(),
            coverage: TargetCoverage::Namespace(coverage),
            evidence_ids: BTreeSet::from([proof.receipt().id.clone()]),
            error: None,
        };
        let storage = persistence_source::analysis_results::write_analysis_result(
            context.case_root,
            proof.receipt(),
            &details,
        )?;
        proof.validate().map_err(super::proof_state::classify)?;
        let analysis = LayerAnalysisResult {
            profile_id: self.profile.id().into(),
            source_revision: parameters.source,
            details,
            receipt: proof.receipt().clone(),
            storage,
        };
        Ok(ActivityOutput {
            proof: Arc::new(ActivityProof::Snapshot(Box::new(proof))),
            capabilities: vec![],
            target_results: vec![result],
            analysis: Some(analysis),
        })
    }
}

fn validate_output_budget(details: &serde_json::Value, limit: u64) -> Result<(), RuntimeError> {
    let bytes = serde_json::to_vec(details).map_err(|_| RuntimeError::InvalidOutput)?;
    if bytes.len() as u64 > limit {
        return Err(RuntimeError::Target(
            domain::structure::TargetFailure::ResourceLimit {
                resource: domain::structure::ResourceKind::Bytes,
                required: bytes.len() as u64,
                limit,
            },
        ));
    }
    Ok(())
}
