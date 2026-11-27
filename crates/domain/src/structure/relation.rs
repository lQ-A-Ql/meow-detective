use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{ByteRange, ByteRangeSet};

pub const MAX_GRAPH_NODES: usize = 4096;
pub const MAX_GRAPH_EDGES: usize = 16384;
pub const MAX_GRAPH_MAPPINGS: usize = 4096;
pub const MAX_GRAPH_RECOVERY_RELATIONS: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StructureNodeId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructureNodeKind {
    SourceRevision,
    StorageObject,
    ObjectVersion,
    Partition,
    FileSystem,
    Workload,
    Namespace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureNode {
    pub id: StructureNodeId,
    pub kind: StructureNodeKind,
    pub label: String,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructureRelationKind {
    DeploymentControl,
    ContentDependency,
    RecoveryGroup,
    Observation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureEdge {
    pub from: StructureNodeId,
    pub to: StructureNodeId,
    pub kind: StructureRelationKind,
    pub role: String,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddressSpace {
    PhysicalByte,
    LogicalBlock,
    File,
    Object,
    Key,
    Namespace,
    Event,
    RuntimeState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressMapping {
    pub id: String,
    pub source_node: StructureNodeId,
    pub target_node: StructureNodeId,
    pub input_space: AddressSpace,
    pub output_space: AddressSpace,
    pub unit: String,
    pub input_range: Option<ByteRange>,
    pub output_range: Option<ByteRange>,
    pub mapping_sha256: String,
    pub adapter_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryHelper {
    pub node: StructureNodeId,
    pub role: String,
    pub slot: u32,
    pub required: bool,
    pub range: Option<ByteRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryGroup {
    pub id: String,
    pub helpers: Vec<RecoveryHelper>,
    pub required_helpers: u32,
    pub algorithm_id: String,
    pub algorithm_version: String,
    pub effective_ranges: Vec<ByteRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryRelation {
    pub id: String,
    pub target_node: StructureNodeId,
    pub groups: Vec<RecoveryGroup>,
    pub verification: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageReport {
    pub target_node: StructureNodeId,
    pub covered: ByteRangeSet,
    pub complete: bool,
    pub missing: Vec<ByteRange>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureGraph {
    pub nodes: Vec<StructureNode>,
    pub edges: Vec<StructureEdge>,
    pub mappings: Vec<AddressMapping>,
    pub recovery_relations: Vec<RecoveryRelation>,
}

impl StructureGraph {
    pub fn validate(&self) -> Result<(), StructureGraphError> {
        validate_bounds(self)?;
        let nodes = self
            .nodes
            .iter()
            .map(|node| (&node.id, node))
            .collect::<BTreeMap<_, _>>();
        if nodes.len() != self.nodes.len() {
            return Err(StructureGraphError::DuplicateNode);
        }
        let mut edges = BTreeSet::new();
        for edge in &self.edges {
            if !nodes.contains_key(&edge.from) || !nodes.contains_key(&edge.to) {
                return Err(StructureGraphError::MissingNode(edge.from.clone()));
            }
            let key = (&edge.from, &edge.to, edge.kind, &edge.role);
            if !edges.insert(key) {
                return Err(StructureGraphError::DuplicateEdge);
            }
        }
        for mapping in &self.mappings {
            if !nodes.contains_key(&mapping.source_node)
                || !nodes.contains_key(&mapping.target_node)
            {
                return Err(StructureGraphError::MissingNode(
                    mapping.source_node.clone(),
                ));
            }
        }
        for relation in &self.recovery_relations {
            if !nodes.contains_key(&relation.target_node) {
                return Err(StructureGraphError::MissingNode(
                    relation.target_node.clone(),
                ));
            }
            for group in &relation.groups {
                if group.required_helpers == 0
                    || group.required_helpers > group.helpers.len() as u32
                {
                    return Err(StructureGraphError::InvalidRecoveryGroup(group.id.clone()));
                }
                for helper in &group.helpers {
                    if !nodes.contains_key(&helper.node) {
                        return Err(StructureGraphError::MissingNode(helper.node.clone()));
                    }
                }
            }
        }
        self.validate_content_dag(&nodes)
    }

    pub fn content_dependency_order(&self) -> Result<Vec<StructureNodeId>, StructureGraphError> {
        self.validate()?;
        let mut incoming = self
            .nodes
            .iter()
            .map(|node| (node.id.clone(), 0usize))
            .collect::<BTreeMap<_, _>>();
        let mut outgoing = BTreeMap::<StructureNodeId, Vec<StructureNodeId>>::new();
        for edge in self
            .edges
            .iter()
            .filter(|edge| edge.kind == StructureRelationKind::ContentDependency)
        {
            *incoming
                .get_mut(&edge.to)
                .ok_or_else(|| StructureGraphError::MissingNode(edge.to.clone()))? += 1;
            outgoing
                .entry(edge.from.clone())
                .or_default()
                .push(edge.to.clone());
        }
        let mut ready = incoming
            .iter()
            .filter_map(|(node, count)| (*count == 0).then_some(node.clone()))
            .collect::<BTreeSet<_>>();
        let mut order = Vec::with_capacity(self.nodes.len());
        while let Some(node) = ready.pop_first() {
            order.push(node.clone());
            if let Some(targets) = outgoing.get(&node) {
                for target in targets {
                    let count = incoming
                        .get_mut(target)
                        .ok_or_else(|| StructureGraphError::MissingNode(target.clone()))?;
                    *count -= 1;
                    if *count == 0 {
                        ready.insert(target.clone());
                    }
                }
            }
        }
        if order.len() != self.nodes.len() {
            return Err(StructureGraphError::DependencyCycle);
        }
        Ok(order)
    }

    fn validate_content_dag(
        &self,
        nodes: &BTreeMap<&StructureNodeId, &StructureNode>,
    ) -> Result<(), StructureGraphError> {
        let mut outgoing = BTreeMap::<&StructureNodeId, Vec<&StructureNodeId>>::new();
        for edge in self
            .edges
            .iter()
            .filter(|edge| edge.kind == StructureRelationKind::ContentDependency)
        {
            outgoing.entry(&edge.from).or_default().push(&edge.to);
        }
        let mut states = BTreeMap::<&StructureNodeId, u8>::new();
        for node in nodes.keys() {
            visit(node, &outgoing, &mut states)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StructureGraphError {
    #[error("structure graph exceeds its node bound")]
    TooManyNodes,
    #[error("structure graph exceeds its edge bound")]
    TooManyEdges,
    #[error("structure graph exceeds its mapping bound")]
    TooManyMappings,
    #[error("structure graph exceeds its recovery-relation bound")]
    TooManyRecoveryRelations,
    #[error("structure graph contains a duplicate node")]
    DuplicateNode,
    #[error("structure graph contains a duplicate edge")]
    DuplicateEdge,
    #[error("structure graph references a missing node: {0:?}")]
    MissingNode(StructureNodeId),
    #[error("content dependency graph contains a cycle")]
    DependencyCycle,
    #[error("recovery group is invalid: {0}")]
    InvalidRecoveryGroup(String),
}

fn validate_bounds(graph: &StructureGraph) -> Result<(), StructureGraphError> {
    if graph.nodes.len() > MAX_GRAPH_NODES {
        return Err(StructureGraphError::TooManyNodes);
    }
    if graph.edges.len() > MAX_GRAPH_EDGES {
        return Err(StructureGraphError::TooManyEdges);
    }
    if graph.mappings.len() > MAX_GRAPH_MAPPINGS {
        return Err(StructureGraphError::TooManyMappings);
    }
    if graph.recovery_relations.len() > MAX_GRAPH_RECOVERY_RELATIONS {
        return Err(StructureGraphError::TooManyRecoveryRelations);
    }
    Ok(())
}

fn visit<'a>(
    node: &'a StructureNodeId,
    outgoing: &BTreeMap<&'a StructureNodeId, Vec<&'a StructureNodeId>>,
    states: &mut BTreeMap<&'a StructureNodeId, u8>,
) -> Result<(), StructureGraphError> {
    match states.get(node).copied().unwrap_or(0) {
        2 => return Ok(()),
        1 => return Err(StructureGraphError::DependencyCycle),
        _ => {}
    }
    states.insert(node, 1);
    if let Some(targets) = outgoing.get(node) {
        for target in targets {
            visit(target, outgoing, states)?;
        }
    }
    states.insert(node, 2);
    Ok(())
}
