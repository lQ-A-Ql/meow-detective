use std::collections::{BTreeMap, BTreeSet};

use super::operation::{
    NativeAlgorithmRef, NativeMappingRef, OperationInput, OperationModelError, OrderingRequirement,
    ReadOperation, ReadOperationNode, ReadOperationPlan, WorkIdentity,
};
use super::{AddressSpace, ByteRange};

pub(super) fn validate_operation(operation: &ReadOperation) -> Result<(), OperationModelError> {
    validate_inputs(operation)?;
    match operation {
        ReadOperation::ReadBackingRange { backing, range } => {
            backing.validate()?;
            validate_range(*range)?;
        }
        ReadOperation::Window {
            input,
            output_range,
        } => {
            validate_range(*output_range)?;
            if input.range.length() != output_range.length() {
                return Err(OperationModelError::InvalidRange);
            }
        }
        ReadOperation::MapExtents { mapping, inputs }
        | ReadOperation::Interleave { mapping, inputs }
        | ReadOperation::ResolveReference { mapping, inputs } => {
            validate_mapping(mapping)?;
            require_inputs(inputs)?;
        }
        ReadOperation::SelectReplica { witness_sha256, .. } => validate_digest(witness_sha256)?,
        ReadOperation::RecoverRange { helpers, algorithm } => {
            require_inputs(helpers)?;
            validate_algorithm(algorithm)?;
        }
        ReadOperation::DecodeContent {
            inputs,
            codec,
            transform_identity,
        } => {
            require_inputs(inputs)?;
            validate_algorithm(codec)?;
            require_text(transform_identity, "transform_identity")?;
        }
        ReadOperation::ApplyOverlay {
            ordered_layers,
            algorithm,
        } => {
            require_inputs(ordered_layers)?;
            validate_algorithm(algorithm)?;
        }
        ReadOperation::LookupNamespace {
            namespace_snapshot,
            native_id,
            inputs,
        } => {
            require_text(namespace_snapshot, "namespace_snapshot")?;
            require_text(native_id, "native_id")?;
            require_inputs(inputs)?;
        }
        ReadOperation::VerifyUnit {
            expected_sha256, ..
        } => {
            if let Some(digest) = expected_sha256 {
                validate_digest(digest)?;
            }
        }
    }
    Ok(())
}

pub(super) fn validate_plan(plan: &ReadOperationPlan) -> Result<(), OperationModelError> {
    require_text(&plan.view_id, "view_id")?;
    require_text(&plan.target_id, "target_id")?;
    require_text(&plan.root_operation_id, "root_operation_id")?;
    if plan.operations.is_empty() || plan.operations.len() > 4096 {
        return Err(OperationModelError::PlanLimit);
    }
    let mut nodes = BTreeMap::new();
    for node in &plan.operations {
        require_text(&node.id, "operation_id")?;
        validate_range(node.output_range)?;
        node.operation.validate()?;
        if nodes.insert(node.id.as_str(), node).is_some() {
            return Err(OperationModelError::DuplicateOperation);
        }
    }
    if !nodes.contains_key(plan.root_operation_id.as_str()) {
        return Err(OperationModelError::MissingDependency);
    }
    for node in &plan.operations {
        validate_node_dependencies(node, &nodes)?;
        validate_node_output(node)?;
        validate_mapping_view(&node.operation, &plan.view_id)?;
    }
    validate_dependency_graph(plan)
}

fn validate_inputs(operation: &ReadOperation) -> Result<(), OperationModelError> {
    let mut references = BTreeSet::new();
    let mut helpers = BTreeSet::new();
    for input in operation.inputs() {
        validate_input(input)?;
        if !references.insert((&input.operation_id, &input.role, input.range)) {
            return Err(OperationModelError::DuplicateInput);
        }
        if matches!(operation, ReadOperation::RecoverRange { .. })
            && !helpers.insert((&input.operation_id, input.range))
        {
            return Err(OperationModelError::DuplicateRecoveryHelper);
        }
        if references.len() > 16_384 {
            return Err(OperationModelError::PlanLimit);
        }
    }
    Ok(())
}

fn validate_node_dependencies(
    node: &ReadOperationNode,
    nodes: &BTreeMap<&str, &ReadOperationNode>,
) -> Result<(), OperationModelError> {
    for input in node.operation.inputs() {
        let parent = nodes
            .get(input.operation_id.as_str())
            .ok_or(OperationModelError::MissingDependency)?;
        if input.range.start() < parent.output_range.start()
            || input.range.end() > parent.output_range.end()
        {
            return Err(OperationModelError::InvalidRange);
        }
        if matches!(
            &node.operation,
            ReadOperation::VerifyUnit { .. } | ReadOperation::SelectReplica { .. }
        ) && node.output_space != parent.output_space
        {
            return Err(OperationModelError::InvalidOutputSpace);
        }
    }
    Ok(())
}

fn validate_node_output(node: &ReadOperationNode) -> Result<(), OperationModelError> {
    let declared = match &node.operation {
        ReadOperation::ReadBackingRange { range, .. } => {
            if node.output_space != AddressSpace::PhysicalByte {
                return Err(OperationModelError::InvalidOutputSpace);
            }
            Some(*range)
        }
        ReadOperation::Window { output_range, .. } => Some(*output_range),
        ReadOperation::VerifyUnit { input, .. } => Some(input.range),
        ReadOperation::SelectReplica { selected, .. } => Some(selected.range),
        _ => None,
    };
    if declared.is_some_and(|range| range != node.output_range) {
        return Err(OperationModelError::InvalidRange);
    }
    Ok(())
}

fn validate_mapping_view(
    operation: &ReadOperation,
    view_id: &str,
) -> Result<(), OperationModelError> {
    let mapping = match operation {
        ReadOperation::MapExtents { mapping, .. }
        | ReadOperation::Interleave { mapping, .. }
        | ReadOperation::ResolveReference { mapping, .. } => mapping,
        _ => return Ok(()),
    };
    if mapping.view_id != view_id {
        return Err(OperationModelError::MappingViewMismatch);
    }
    Ok(())
}

fn validate_dependency_graph(plan: &ReadOperationPlan) -> Result<(), OperationModelError> {
    use petgraph::{algo::toposort, stable_graph::StableDiGraph, visit::Dfs};
    let mut graph = StableDiGraph::<&str, ()>::new();
    let indices = plan
        .operations
        .iter()
        .map(|node| (node.id.as_str(), graph.add_node(node.id.as_str())))
        .collect::<BTreeMap<_, _>>();
    let mut edges = 0;
    for node in &plan.operations {
        for input in node.operation.inputs() {
            edges += 1;
            if edges > 16_384 {
                return Err(OperationModelError::PlanLimit);
            }
            graph.add_edge(
                indices[node.id.as_str()],
                indices[input.operation_id.as_str()],
                (),
            );
        }
    }
    toposort(&graph, None).map_err(|_| OperationModelError::DependencyCycle)?;
    let mut visitor = Dfs::new(&graph, indices[plan.root_operation_id.as_str()]);
    let mut visited = 0;
    while visitor.next(&graph).is_some() {
        visited += 1;
    }
    if visited != plan.operations.len() {
        return Err(OperationModelError::UnusedOperation);
    }
    Ok(())
}

pub(super) fn validate_work(work: &WorkIdentity) -> Result<(), OperationModelError> {
    for (value, field) in [
        (&work.operation_id, "operation_id"),
        (&work.adapter_id, "adapter_id"),
        (&work.adapter_version, "adapter_version"),
        (&work.goal_id, "goal_id"),
        (&work.policy_version, "policy_version"),
    ] {
        require_text(value, field)?;
    }
    if work.input_capability_versions.is_empty() {
        return Err(OperationModelError::MissingInputs);
    }
    for reference in &work.input_capability_versions {
        require_text(reference, "input_capability_version")?;
    }
    for digest in [
        &work.selector_sha256,
        &work.version_witness_sha256,
        &work.scope_witness_sha256,
    ] {
        validate_digest(digest)?;
    }
    validate_ordering(&work.ordering)
}

pub(super) fn validate_ordering(ordering: &OrderingRequirement) -> Result<(), OperationModelError> {
    match ordering {
        OrderingRequirement::NativeSequence {
            domain,
            boundary_sha256,
        } => {
            require_text(domain, "sequence_domain")?;
            validate_digest(boundary_sha256)?;
        }
        OrderingRequirement::NamespaceKey { snapshot, .. } => {
            require_text(snapshot, "snapshot")?;
        }
        OrderingRequirement::Unordered => {}
    }
    Ok(())
}

fn validate_mapping(mapping: &NativeMappingRef) -> Result<(), OperationModelError> {
    require_text(&mapping.view_id, "view_id")?;
    require_text(&mapping.adapter_id, "adapter_id")?;
    require_text(&mapping.adapter_version, "adapter_version")?;
    validate_digest(&mapping.mapping_sha256)?;
    validate_digest(&mapping.version_witness_sha256)
}

fn validate_algorithm(algorithm: &NativeAlgorithmRef) -> Result<(), OperationModelError> {
    require_text(&algorithm.id, "algorithm_id")?;
    require_text(&algorithm.version, "algorithm_version")?;
    validate_digest(&algorithm.parameters_sha256)?;
    validate_digest(&algorithm.version_witness_sha256)
}

fn validate_input(input: &OperationInput) -> Result<(), OperationModelError> {
    require_text(&input.operation_id, "input_operation_id")?;
    require_text(&input.role, "input_role")?;
    validate_range(input.range)
}

fn validate_range(range: ByteRange) -> Result<(), OperationModelError> {
    range
        .start()
        .checked_add(range.length())
        .ok_or(OperationModelError::InvalidRange)?;
    Ok(())
}

fn validate_digest(value: &str) -> Result<(), OperationModelError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(OperationModelError::InvalidDigest);
    }
    Ok(())
}

fn require_inputs(inputs: &[OperationInput]) -> Result<(), OperationModelError> {
    if inputs.is_empty() {
        return Err(OperationModelError::MissingInputs);
    }
    Ok(())
}

fn require_text(value: &str, field: &'static str) -> Result<(), OperationModelError> {
    if value.trim().is_empty() {
        return Err(OperationModelError::EmptyField(field));
    }
    Ok(())
}
