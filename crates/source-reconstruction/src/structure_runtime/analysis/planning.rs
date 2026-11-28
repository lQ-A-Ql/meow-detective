use std::collections::BTreeSet;

use domain::structure::{
    canonical_identity, CapabilityIntegrity, CapabilityKind, CapabilityRef, CoverageContext,
    KeyBoundary, KeyCoverage, KeyOrdering, NamespaceCoverage, NamespaceOrdering, NamespaceQuery,
    NativeKeyRange, OrderingRequirement, SourceCaptureRef, SourceRevisionRef, StructureGoal,
    TargetCoverage, TargetSelector, VersionBoundary, VersionSelection, WorkIdentity,
};
use persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo;
use serde::{Deserialize, Serialize};

use super::super::{
    activity::identity_digest, frontier::CapabilityEntry, ActivityDefinition, BindingContext,
    BindingDecision, ExecutableDescriptor, RuntimeError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerAnalysisProfile {
    CephOsdMetadata,
    CephFsNamespace,
    KubernetesMetadata,
    GuestLinuxArtifacts,
}

impl LayerAnalysisProfile {
    pub fn id(self) -> &'static str {
        match self {
            Self::CephOsdMetadata => "ceph-osd-metadata",
            Self::CephFsNamespace => "cephfs-namespace",
            Self::KubernetesMetadata => "kubernetes-metadata",
            Self::GuestLinuxArtifacts => "guest-linux-artifacts",
        }
    }

    pub(super) fn input_kind(self) -> CapabilityKind {
        match self {
            Self::CephOsdMetadata => CapabilityKind::MemberMetadata,
            Self::CephFsNamespace => CapabilityKind::Namespace,
            Self::KubernetesMetadata => CapabilityKind::ResourceRecords,
            Self::GuestLinuxArtifacts => CapabilityKind::Namespace,
        }
    }

    pub(super) fn role(self) -> &'static str {
        self.id()
    }

    pub(super) fn requirements(self, view_id: &str) -> Vec<super::super::AnalysisInputRequirement> {
        let kinds = match self {
            Self::CephOsdMetadata => {
                vec![CapabilityKind::MemberMetadata, CapabilityKind::KvInventory]
            }
            _ => vec![self.input_kind()],
        };
        kinds
            .into_iter()
            .map(|kind| super::super::AnalysisInputRequirement {
                kind,
                native_role: format!("{}:{view_id}", self.role()),
                minimum_integrity: super::super::MinimumIntegrity::Observed,
            })
            .collect()
    }

    pub(super) fn required_capabilities(self, view_id: &str) -> BTreeSet<CapabilityKind> {
        let mut kinds = self
            .requirements(view_id)
            .iter()
            .map(|requirement| requirement.kind)
            .collect::<BTreeSet<_>>();
        if self == Self::GuestLinuxArtifacts {
            kinds.insert(CapabilityKind::FileRange);
        }
        kinds
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct AnalysisParameters {
    pub source: SourceRevisionRef,
    pub native_id: String,
    pub generation_identity: String,
}

pub fn analysis_source_seeds(
    connection: &rusqlite::Connection,
    root: &std::path::Path,
    capture: &SourceCaptureRef,
    scope_id: &str,
    profile: LayerAnalysisProfile,
    native_id: &str,
) -> Result<Vec<CapabilityEntry>, RuntimeError> {
    capture
        .validate()
        .map_err(|_| RuntimeError::InvalidDefinition)?;
    SourceRevisionRepo::new(connection).require_current(&capture.source)?;
    let (_, content) = SourceRevisionRepo::new(connection)
        .current(&capture.source.case_id, &capture.source.source_id)?;
    if content.acquisition_id != capture.acquisition_id || native_id.trim().is_empty() {
        return Err(RuntimeError::InvalidDefinition);
    }
    let generation = generation_identity(connection, root, &capture.source)?;
    let view_id = canonical_identity(
        "analysis-source-view-v1",
        &(capture, profile.id(), native_id, &generation),
    )?;
    let context = CoverageContext {
        view_id: view_id.clone(),
        native_namespace: profile.id().into(),
        version: VersionBoundary {
            adapter_id: profile.id().into(),
            native_kind: "published-generation".into(),
            native_value: generation,
            witness_sha256: identity_digest("analysis-source-version-v1", capture)?,
        },
    };
    let target = selector(&context, native_id);
    let mut entries = Vec::new();
    for requirement in profile.requirements(&view_id) {
        let coverage = if requirement.kind == CapabilityKind::KvInventory {
            let range = NativeKeyRange {
                lower: KeyBoundary::UnboundedStart,
                upper: KeyBoundary::UnboundedEnd,
            };
            TargetCoverage::Keys(KeyCoverage {
                context: context.clone(),
                ordering: KeyOrdering {
                    comparator_id: "published-native-kv-inventory".into(),
                    sort_key_encoding_id: "native-sort-key-witness".into(),
                    witness_sha256: context.version.witness_sha256.clone(),
                },
                requested: vec![range.clone()],
                verified: vec![],
                tombstones: vec![],
                unknown: vec![range],
                unreadable: vec![],
            })
        } else {
            TargetCoverage::Namespace(NamespaceCoverage {
                context: context.clone(),
                root_id: native_id.into(),
                query_sha256: identity_digest("analysis-query-v1", profile.id())?,
                ordering_id: "analysis-summary-v1".into(),
                pages: vec![],
            })
        };
        entries.push(CapabilityEntry {
            capability: CapabilityRef::new(
                target
                    .identity()
                    .map_err(|_| RuntimeError::InvalidDefinition)?,
                view_id.clone(),
                requirement.kind,
                capture.source.revision,
                CapabilityIntegrity::Observed,
                coverage,
            )
            .map_err(|_| RuntimeError::InvalidOutput)?,
            scope_id: scope_id.into(),
            native_roles: BTreeSet::from([profile.role().into(), requirement.native_role]),
        });
    }
    Ok(entries)
}

pub(super) fn generation_identity(
    connection: &rusqlite::Connection,
    root: &std::path::Path,
    source: &SourceRevisionRef,
) -> Result<String, RuntimeError> {
    let control = source_kernel::source_db::open_registered_source_db_read_only(
        connection,
        root,
        &domain::DataSourceId(source.source_id.clone()),
    )?;
    let generation_name = persistence_source::detail::generation_name(&control)?;
    let native = source_kernel::source_db::open_registered_fjall_source_read_only(
        root,
        &domain::DataSourceId(source.source_id.clone()),
    )
    .map_err(|error| RuntimeError::Io(std::io::Error::other(error)))?;
    let manifest = native
        .generation_manifest(&source.source_id)
        .map_err(|_| RuntimeError::InvalidDefinition)?;
    Ok(canonical_identity(
        "analysis-input-generation-v1",
        &(generation_name, manifest),
    )?)
}

pub(super) fn bind(
    profile: LayerAnalysisProfile,
    descriptor: ExecutableDescriptor,
    context: &BindingContext<'_>,
) -> Result<BindingDecision, RuntimeError> {
    if !context.request.goals.contains(&StructureGoal::Analyze)
        || !context.request.analysis_profiles.contains(profile.id())
    {
        return Ok(BindingDecision::NotMatched);
    }
    let entry = context.changed;
    let inputs = match_profile_inputs(profile, context)?;
    let TargetCoverage::Namespace(coverage) = &entry.capability.coverage else {
        return Err(RuntimeError::InvalidDefinition);
    };
    let capture = context
        .request
        .source_scope
        .iter()
        .find(|capture| {
            canonical_identity(
                "analysis-source-view-v1",
                &(
                    capture,
                    profile.id(),
                    &coverage.root_id,
                    &coverage.context.version.native_value,
                ),
            )
            .is_ok_and(|view| view == entry.capability.view_id)
        })
        .ok_or(RuntimeError::InvalidDefinition)?;
    let target = selector(&coverage.context, &coverage.root_id);
    match &context.request.version_selection {
        VersionSelection::Current => {}
        VersionSelection::Exact { view_id } if view_id == target.view_id() => {}
        VersionSelection::Exact { .. } => return Ok(BindingDecision::NotMatched),
        _ => return Err(RuntimeError::InvalidDefinition),
    }
    if !context.request.targets.is_empty() && !context.request.targets.contains(&target) {
        return Ok(BindingDecision::NotMatched);
    }
    let mut output_coverage = coverage.clone();
    output_coverage.context.native_namespace = format!("analysis-results:{}", profile.id());
    output_coverage.query_sha256 = identity_digest("analysis-result-query-v1", &target)?;
    output_coverage.pages.clear();
    let definition = ActivityDefinition {
        work: WorkIdentity {
            operation_id: descriptor.operation_id,
            adapter_id: descriptor.adapter_id,
            adapter_version: descriptor.adapter_version,
            goal_id: profile.id().into(),
            input_capability_versions: inputs.capability_ids.clone(),
            selector_sha256: identity_digest("target-selector-v1", &target)?,
            version_witness_sha256: identity_digest("analysis-capability-v1", &entry.capability)?,
            scope_witness_sha256: identity_digest("analysis-scope-v1", &entry.scope_id)?,
            ordering: OrderingRequirement::Unordered,
            policy_version: "layer-analysis-v1".into(),
        },
        stage: descriptor.stage,
        target,
        scope_id: entry.scope_id.clone(),
        required_capabilities: profile.required_capabilities(&entry.capability.view_id),
        requested_coverage: TargetCoverage::Namespace(output_coverage),
        dependencies: BTreeSet::new(),
        parameters: serde_json::to_value(AnalysisParameters {
            source: capture.source.clone(),
            native_id: coverage.root_id.clone(),
            generation_identity: coverage.context.version.native_value.clone(),
        })
        .map_err(|_| RuntimeError::InvalidDefinition)?,
    };
    if !inputs.missing.is_empty() {
        let missing = inputs
            .missing
            .iter()
            .map(|prerequisite| prerequisite.kind)
            .collect();
        let result =
            definition.failure_result(domain::structure::TargetFailure::MissingCapabilities {
                capabilities: missing,
            })?;
        return Ok(BindingDecision::RequiresEvidence {
            candidate_id: canonical_identity(
                "layer-analysis-candidate-v1",
                &(profile.id(), &entry.capability.view_id),
            )?,
            prerequisites: inputs.missing,
            result: Box::new(result),
        });
    }
    Ok(BindingDecision::Bound(vec![definition]))
}

fn match_profile_inputs(
    profile: LayerAnalysisProfile,
    context: &BindingContext<'_>,
) -> Result<super::super::AnalysisInputMatch, RuntimeError> {
    super::super::match_inputs(
        &context.changed.scope_id,
        &profile.requirements(&context.changed.capability.view_id),
        context.capabilities,
    )
    .map_err(|_| RuntimeError::InvalidDefinition)
}

fn selector(context: &CoverageContext, native_id: &str) -> TargetSelector {
    TargetSelector::Namespace {
        view_id: context.view_id.clone(),
        namespace_snapshot: context.version.native_value.clone(),
        root_id: native_id.into(),
        query: NamespaceQuery {
            filters: BTreeSet::new(),
            fields: BTreeSet::from(["native-summary".into()]),
            ordering: NamespaceOrdering::Native {
                ordering_id: "analysis-summary-v1".into(),
            },
            page_size: 1,
            cursor: None,
        },
    }
}
