use std::collections::{BTreeMap, BTreeSet};

use domain::structure::{CapabilityKind, StructureRequest, StructureStage};

use super::{
    frontier::{BindingPrerequisite, CapabilityEntry, CapabilityIndex},
    ActivityDefinition, ActivityOutput, ExecutionContext, RuntimeError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutableDescriptor {
    pub adapter_id: String,
    pub adapter_version: String,
    pub operation_id: String,
    pub stage: StructureStage,
    pub input_kind: CapabilityKind,
    pub input_role: String,
}

impl ExecutableDescriptor {
    pub fn validate(&self) -> Result<(), RuntimeError> {
        if [
            &self.adapter_id,
            &self.adapter_version,
            &self.operation_id,
            &self.input_role,
        ]
        .iter()
        .any(|value| value.trim().is_empty())
        {
            return Err(RuntimeError::InvalidRegistration);
        }
        Ok(())
    }
}

pub struct BindingContext<'a> {
    pub request: &'a StructureRequest,
    pub changed: &'a CapabilityEntry,
    pub capabilities: &'a CapabilityIndex,
}

pub enum BindingDecision {
    NotMatched,
    Rejected(Vec<domain::structure::TargetResult>),
    RequiresEvidence {
        candidate_id: String,
        prerequisites: BTreeSet<BindingPrerequisite>,
        result: Box<domain::structure::TargetResult>,
    },
    Bound(Vec<ActivityDefinition>),
}

pub trait StructureOperation: Send + Sync {
    fn descriptor(&self) -> ExecutableDescriptor;
    fn bind(&self, context: &BindingContext<'_>) -> Result<BindingDecision, RuntimeError>;
    fn execute(
        &self,
        activity: &ActivityDefinition,
        context: &ExecutionContext<'_>,
    ) -> Result<ActivityOutput, RuntimeError>;
}

pub struct ExecutableRegistry {
    limit: usize,
    operations: BTreeMap<(String, String), RegisteredOperation>,
}

struct RegisteredOperation {
    descriptor: ExecutableDescriptor,
    operation: Box<dyn StructureOperation>,
}

impl StructureOperation for RegisteredOperation {
    fn descriptor(&self) -> ExecutableDescriptor {
        self.descriptor.clone()
    }

    fn bind(&self, context: &BindingContext<'_>) -> Result<BindingDecision, RuntimeError> {
        self.operation.bind(context)
    }

    fn execute(
        &self,
        activity: &ActivityDefinition,
        context: &ExecutionContext<'_>,
    ) -> Result<ActivityOutput, RuntimeError> {
        self.operation.execute(activity, context)
    }
}

impl ExecutableRegistry {
    pub fn new(limit: usize) -> Result<Self, RuntimeError> {
        if limit == 0 {
            return Err(RuntimeError::Capacity);
        }
        Ok(Self {
            limit,
            operations: BTreeMap::new(),
        })
    }

    pub fn register(&mut self, operation: Box<dyn StructureOperation>) -> Result<(), RuntimeError> {
        let descriptor = operation.descriptor();
        descriptor.validate()?;
        let key = (
            descriptor.adapter_id.clone(),
            descriptor.operation_id.clone(),
        );
        if self.operations.contains_key(&key) {
            return Err(RuntimeError::DuplicateRegistration);
        }
        if self.operations.len() >= self.limit {
            return Err(RuntimeError::Capacity);
        }
        self.operations.insert(
            key,
            RegisteredOperation {
                descriptor,
                operation,
            },
        );
        Ok(())
    }

    pub fn get(&self, adapter_id: &str, operation_id: &str) -> Option<&dyn StructureOperation> {
        self.operations
            .get(&(adapter_id.to_owned(), operation_id.to_owned()))
            .map(|registered| registered as &dyn StructureOperation)
    }

    pub fn matching(&self, entry: &CapabilityEntry) -> Vec<&dyn StructureOperation> {
        self.operations
            .values()
            .filter(|operation| {
                let descriptor = operation.descriptor();
                descriptor.input_kind == entry.capability.kind
                    && entry.native_roles.contains(&descriptor.input_role)
            })
            .map(|registered| registered as &dyn StructureOperation)
            .collect()
    }

    pub fn descriptors(&self) -> Vec<ExecutableDescriptor> {
        self.operations
            .values()
            .map(|operation| operation.descriptor())
            .collect()
    }
}
