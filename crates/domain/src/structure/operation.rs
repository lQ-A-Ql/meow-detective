use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize};

use super::{AddressSpace, BackingVersionRef, ByteRange, ReadVerification};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeMappingRef {
    pub view_id: String,
    pub adapter_id: String,
    pub adapter_version: String,
    pub mapping_sha256: String,
    pub version_witness_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationInput {
    pub operation_id: String,
    pub role: String,
    pub range: ByteRange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeAlgorithmRef {
    pub id: String,
    pub version: String,
    pub parameters_sha256: String,
    pub version_witness_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ReadOperation {
    ReadBackingRange {
        backing: BackingVersionRef,
        range: ByteRange,
    },
    Window {
        input: OperationInput,
        output_range: ByteRange,
    },
    MapExtents {
        mapping: NativeMappingRef,
        inputs: Vec<OperationInput>,
    },
    Interleave {
        mapping: NativeMappingRef,
        inputs: Vec<OperationInput>,
    },
    SelectReplica {
        selected: OperationInput,
        witness_sha256: String,
    },
    RecoverRange {
        helpers: Vec<OperationInput>,
        algorithm: NativeAlgorithmRef,
    },
    ResolveReference {
        mapping: NativeMappingRef,
        inputs: Vec<OperationInput>,
    },
    DecodeContent {
        inputs: Vec<OperationInput>,
        codec: NativeAlgorithmRef,
        transform_identity: String,
    },
    ApplyOverlay {
        ordered_layers: Vec<OperationInput>,
        algorithm: NativeAlgorithmRef,
    },
    LookupNamespace {
        namespace_snapshot: String,
        native_id: String,
        inputs: Vec<OperationInput>,
    },
    VerifyUnit {
        input: OperationInput,
        method: ReadVerification,
        expected_sha256: Option<String>,
    },
}

impl ReadOperation {
    pub fn inputs(&self) -> Vec<&OperationInput> {
        match self {
            Self::ReadBackingRange { .. } => Vec::new(),
            Self::Window { input, .. } | Self::VerifyUnit { input, .. } => vec![input],
            Self::SelectReplica { selected, .. } => vec![selected],
            Self::MapExtents { inputs, .. }
            | Self::Interleave { inputs, .. }
            | Self::ResolveReference { inputs, .. }
            | Self::DecodeContent { inputs, .. }
            | Self::LookupNamespace { inputs, .. } => inputs.iter().collect(),
            Self::RecoverRange { helpers, .. } => helpers.iter().collect(),
            Self::ApplyOverlay { ordered_layers, .. } => ordered_layers.iter().collect(),
        }
    }

    pub fn validate(&self) -> Result<(), OperationModelError> {
        super::operation_validation::validate_operation(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadOperationNode {
    pub id: String,
    pub output_space: AddressSpace,
    pub output_range: ByteRange,
    pub operation: ReadOperation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadOperationPlan {
    pub view_id: String,
    pub target_id: String,
    pub root_operation_id: String,
    pub operations: Vec<ReadOperationNode>,
}

impl ReadOperationPlan {
    pub fn validate(&self) -> Result<(), OperationModelError> {
        super::operation_validation::validate_plan(self)
    }
}

impl<'de> Deserialize<'de> for ReadOperationPlan {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(
            remote = "ReadOperationPlan",
            rename_all = "camelCase",
            deny_unknown_fields
        )]
        struct Record {
            view_id: String,
            target_id: String,
            root_operation_id: String,
            operations: Vec<ReadOperationNode>,
        }
        let plan = Record::deserialize(deserializer)?;
        plan.validate().map_err(serde::de::Error::custom)?;
        Ok(plan)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum OrderingRequirement {
    Unordered,
    NativeSequence {
        domain: String,
        boundary_sha256: String,
    },
    NamespaceKey {
        snapshot: String,
        descending: bool,
    },
}

impl<'de> Deserialize<'de> for OrderingRequirement {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(
            tag = "kind",
            rename_all = "snake_case",
            rename_all_fields = "camelCase",
            deny_unknown_fields
        )]
        enum Record {
            Unordered {},
            NativeSequence {
                domain: String,
                boundary_sha256: String,
            },
            NamespaceKey {
                snapshot: String,
                descending: bool,
            },
        }
        let ordering = match Record::deserialize(deserializer)? {
            Record::Unordered {} => Self::Unordered,
            Record::NativeSequence {
                domain,
                boundary_sha256,
            } => Self::NativeSequence {
                domain,
                boundary_sha256,
            },
            Record::NamespaceKey {
                snapshot,
                descending,
            } => Self::NamespaceKey {
                snapshot,
                descending,
            },
        };
        super::operation_validation::validate_ordering(&ordering)
            .map_err(serde::de::Error::custom)?;
        Ok(ordering)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkIdentity {
    pub operation_id: String,
    pub adapter_id: String,
    pub adapter_version: String,
    pub goal_id: String,
    pub input_capability_versions: BTreeSet<String>,
    pub selector_sha256: String,
    pub version_witness_sha256: String,
    pub scope_witness_sha256: String,
    pub ordering: OrderingRequirement,
    pub policy_version: String,
}

impl WorkIdentity {
    pub fn validate(&self) -> Result<(), OperationModelError> {
        super::operation_validation::validate_work(self)
    }

    pub fn key(&self) -> Result<String, OperationModelError> {
        self.validate()?;
        Ok(super::canonical_identity("structure-work-v1", self)?)
    }
}

impl<'de> Deserialize<'de> for WorkIdentity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(remote = "WorkIdentity", rename_all = "camelCase", deny_unknown_fields)]
        struct Record {
            operation_id: String,
            adapter_id: String,
            adapter_version: String,
            goal_id: String,
            input_capability_versions: BTreeSet<String>,
            selector_sha256: String,
            version_witness_sha256: String,
            scope_witness_sha256: String,
            ordering: OrderingRequirement,
            policy_version: String,
        }
        let work = Record::deserialize(deserializer)?;
        work.validate().map_err(serde::de::Error::custom)?;
        Ok(work)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OperationModelError {
    #[error("operation field is empty: {0}")]
    EmptyField(&'static str),
    #[error("operation integrity digest is invalid")]
    InvalidDigest,
    #[error("operation range is invalid")]
    InvalidRange,
    #[error("operation inputs are missing")]
    MissingInputs,
    #[error("operation input with the same source, role and range is duplicated")]
    DuplicateInput,
    #[error("recovery helper aliases the same operation and range")]
    DuplicateRecoveryHelper,
    #[error("operation identifier is duplicated")]
    DuplicateOperation,
    #[error("operation output address space does not match its input semantics")]
    InvalidOutputSpace,
    #[error("native mapping belongs to a different view from the operation plan")]
    MappingViewMismatch,
    #[error("operation dependency is absent from the plan")]
    MissingDependency,
    #[error("operation dependency graph contains a cycle")]
    DependencyCycle,
    #[error("operation is not reachable from the selected output")]
    UnusedOperation,
    #[error("operation plan exceeds its bound")]
    PlanLimit,
    #[error(transparent)]
    Backing(#[from] super::ViewModelError),
    #[error(transparent)]
    Identity(#[from] super::CanonicalIdentityError),
}
