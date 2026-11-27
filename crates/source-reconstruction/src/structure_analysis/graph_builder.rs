use std::collections::{BTreeMap, BTreeSet};

use domain::{
    structure::{
        AddressMapping, AddressSpace, ByteRange, RecoveryGroup, RecoveryHelper, RecoveryRelation,
        SourceRevisionContent, SourceRevisionRef, StorageObjectVersion, StructureEdge,
        StructureGraph, StructureNode, StructureNodeId, StructureNodeKind, StructureRelationKind,
    },
    CaseId, DataSource,
};
use evidence_access::partition_query_service::{PartitionQueryRepo, PartitionRecord};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, source_revision_repo::SourceRevisionRepo,
    storage_object_repo::StorageObjectRepo, structure_observation_repo::StructureObservationRepo,
};
use sha2::{Digest, Sha256};

use super::request::StructureAnalysisError;

pub(crate) struct SourceContext {
    pub data_source: DataSource,
    pub revision_ref: SourceRevisionRef,
    pub revision: SourceRevisionContent,
    pub versions: Vec<StorageObjectVersion>,
    pub partitions: Vec<PartitionRecord>,
}

pub(crate) struct GraphBuildResult {
    pub graph: StructureGraph,
    pub sources: Vec<SourceContext>,
}

pub(crate) fn build_structure_graph(
    conn: &rusqlite::Connection,
    _case_root: &std::path::Path,
    case_id: &CaseId,
    requested_source_ids: &[String],
) -> Result<GraphBuildResult, StructureAnalysisError> {
    let sources = DataSourceRepo::new(conn).find_by_case(case_id)?;
    let selected = select_sources(&sources, requested_source_ids)?;
    let revisions = SourceRevisionRepo::new(conn);
    let version_repo = StorageObjectRepo::new(conn);
    let partitions = PartitionQueryRepo::new(conn);
    let observations = StructureObservationRepo::new(conn);
    let source_by_id = sources
        .iter()
        .map(|source| (source.id.0.as_str(), source))
        .collect::<BTreeMap<_, _>>();
    let mut graph = StructureGraph {
        nodes: Vec::new(),
        edges: Vec::new(),
        mappings: Vec::new(),
        recovery_relations: Vec::new(),
    };
    let mut nodes = BTreeMap::<StructureNodeId, StructureNode>::new();
    let mut contexts = Vec::with_capacity(selected.len());
    for source in selected {
        let (revision_ref, revision) = revisions.current(&case_id.0, &source.id.0)?;
        let versions = version_repo.versions_for_source(&case_id.0, &source.id.0)?;
        let source_partitions = partitions.find_by_data_source(&source.id.0)?;
        let observation_count = observations.count_published(&revision_ref)?;
        contexts.push(append_source_graph(
            source,
            revision_ref,
            revision,
            versions,
            source_partitions,
            observation_count,
            &source_by_id,
            &version_repo,
            &mut nodes,
            &mut graph,
        )?);
    }
    graph.nodes = nodes.into_values().collect();
    graph
        .validate()
        .map_err(|error| StructureAnalysisError::Graph(error.to_string()))?;
    Ok(GraphBuildResult {
        graph,
        sources: contexts,
    })
}

#[allow(clippy::too_many_arguments)]
fn append_source_graph(
    source: &DataSource,
    revision_ref: SourceRevisionRef,
    revision: SourceRevisionContent,
    versions: Vec<StorageObjectVersion>,
    source_partitions: Vec<PartitionRecord>,
    observation_count: u64,
    source_by_id: &BTreeMap<&str, &DataSource>,
    version_repo: &StorageObjectRepo<'_>,
    nodes: &mut BTreeMap<StructureNodeId, StructureNode>,
    graph: &mut StructureGraph,
) -> Result<SourceContext, StructureAnalysisError> {
    let source_node = add_source_node(nodes, source, &revision_ref, observation_count);
    for version in &versions {
        let object = version_repo.find(&version.storage_object_id)?;
        let object_node = StructureNodeId(format!("object:{}", version.storage_object_id));
        nodes
            .entry(object_node.clone())
            .or_insert_with(|| StructureNode {
                id: object_node.clone(),
                kind: StructureNodeKind::StorageObject,
                label: object.as_ref().map_or_else(
                    || version.storage_object_id.clone(),
                    |object| object.name.clone(),
                ),
                details: serde_json::json!({
                    "objectKind": object.as_ref().map(|object| object.object_kind.clone()),
                    "identityState": object.as_ref().map(|object| object.identity_state.clone()),
                }),
            });
        let version_node = StructureNodeId(format!("version:{}", version.id.0));
        nodes
            .entry(version_node.clone())
            .or_insert_with(|| StructureNode {
                id: version_node.clone(),
                kind: StructureNodeKind::ObjectVersion,
                label: version.adapter_version.clone(),
                details: serde_json::json!({
                    "storageObjectId": version.storage_object_id,
                    "mappingSha256": version.mapping_sha256,
                    "consistency": version.consistency,
                    "versionKind": version.version_kind,
                }),
            });
        add_edge(
            graph,
            source_node.clone(),
            version_node.clone(),
            StructureRelationKind::Observation,
            "exposes_version",
        );
        add_edge(
            graph,
            version_node.clone(),
            object_node,
            StructureRelationKind::DeploymentControl,
            "defines_object",
        );
        for parent in &version.parent_source_revisions {
            let parent_node = add_parent_source_node(nodes, source_by_id, parent);
            add_edge(
                graph,
                version_node.clone(),
                parent_node,
                StructureRelationKind::ContentDependency,
                "depends_on",
            );
        }
        if version.adapter_version.starts_with("fs-lvm/mdraid") {
            add_mdraid_relations(
                graph,
                source_by_id,
                &version_node,
                version,
                &source.source_path,
                revision.logical_size,
            )?;
        }
    }
    for partition in &source_partitions {
        add_partition_nodes(graph, nodes, source, &source_node, partition);
    }
    Ok(SourceContext {
        data_source: source.clone(),
        revision_ref,
        revision,
        versions,
        partitions: source_partitions,
    })
}

fn select_sources<'a>(
    sources: &'a [DataSource],
    requested: &[String],
) -> Result<Vec<&'a DataSource>, StructureAnalysisError> {
    if requested.is_empty() {
        return Ok(sources.iter().collect());
    }
    let by_id = sources
        .iter()
        .map(|source| (source.id.0.as_str(), source))
        .collect::<BTreeMap<_, _>>();
    let mut seen = BTreeSet::new();
    requested
        .iter()
        .map(|source_id| {
            if !seen.insert(source_id.as_str()) {
                return Err(StructureAnalysisError::MissingSource(source_id.clone()));
            }
            by_id
                .get(source_id.as_str())
                .copied()
                .ok_or_else(|| StructureAnalysisError::MissingSource(source_id.clone()))
        })
        .collect()
}

fn add_source_node(
    nodes: &mut BTreeMap<StructureNodeId, StructureNode>,
    source: &DataSource,
    revision: &SourceRevisionRef,
    observation_count: u64,
) -> StructureNodeId {
    let id = source_node_id(&source.id.0, revision.revision);
    nodes.entry(id.clone()).or_insert_with(|| StructureNode {
        id: id.clone(),
        kind: StructureNodeKind::SourceRevision,
        label: source.name.clone(),
        details: serde_json::json!({
            "sourceId": source.id.0,
            "sourceKind": source.kind,
            "revision": revision.revision,
            "observationCount": observation_count,
        }),
    });
    id
}

fn add_parent_source_node(
    nodes: &mut BTreeMap<StructureNodeId, StructureNode>,
    sources: &BTreeMap<&str, &DataSource>,
    revision: &SourceRevisionRef,
) -> StructureNodeId {
    let id = source_node_id(&revision.source_id, revision.revision);
    let label = sources
        .get(revision.source_id.as_str())
        .map_or_else(|| revision.source_id.clone(), |source| source.name.clone());
    nodes.entry(id.clone()).or_insert_with(|| StructureNode {
        id: id.clone(),
        kind: StructureNodeKind::SourceRevision,
        label,
        details: serde_json::json!({
            "sourceId": revision.source_id,
            "revision": revision.revision,
            "parentOnly": true,
        }),
    });
    id
}

fn add_partition_nodes(
    graph: &mut StructureGraph,
    nodes: &mut BTreeMap<StructureNodeId, StructureNode>,
    source: &DataSource,
    source_node: &StructureNodeId,
    partition: &PartitionRecord,
) -> StructureNodeId {
    let partition_node = StructureNodeId(format!(
        "partition:{}:{}",
        source.id.0, partition.partition_index
    ));
    nodes
        .entry(partition_node.clone())
        .or_insert_with(|| StructureNode {
            id: partition_node.clone(),
            kind: StructureNodeKind::Partition,
            label: partition.name.clone(),
            details: serde_json::json!({
                "partitionIndex": partition.partition_index,
                "offset": partition.offset,
                "length": partition.length,
                "status": partition.status,
                "typeGuid": partition.type_guid,
            }),
        });
    add_edge(
        graph,
        source_node.clone(),
        partition_node.clone(),
        StructureRelationKind::DeploymentControl,
        "contains_partition",
    );
    if let Some(filesystem) = &partition.filesystem {
        let filesystem_node = StructureNodeId(format!(
            "filesystem:{}:{}:{}",
            source.id.0, partition.partition_index, filesystem
        ));
        nodes
            .entry(filesystem_node.clone())
            .or_insert_with(|| StructureNode {
                id: filesystem_node.clone(),
                kind: StructureNodeKind::FileSystem,
                label: filesystem.clone(),
                details: serde_json::json!({
                    "filesystem": filesystem,
                    "filesystemUuid": partition.filesystem_uuid,
                }),
            });
        add_edge(
            graph,
            partition_node.clone(),
            filesystem_node,
            StructureRelationKind::DeploymentControl,
            "contains_filesystem",
        );
    }
    partition_node
}

fn add_mdraid_relations(
    graph: &mut StructureGraph,
    sources: &BTreeMap<&str, &DataSource>,
    version_node: &StructureNodeId,
    version: &StorageObjectVersion,
    descriptor_path: &std::path::Path,
    logical_size: u64,
) -> Result<(), StructureAnalysisError> {
    let plan = evidence_access::mdraid::read_mdraid_descriptor(descriptor_path)?;
    let fingerprint = evidence_access::mdraid::mdraid_array_fingerprint(&plan)?;
    if fingerprint != version.mapping_sha256 {
        return Err(StructureAnalysisError::Graph(
            "MD RAID descriptor does not match the published version".into(),
        ));
    }
    let parent_by_path = version
        .parent_source_revisions
        .iter()
        .filter_map(|parent| {
            sources
                .get(parent.source_id.as_str())
                .map(|source| (source, parent))
        })
        .map(|(source, parent)| {
            std::fs::canonicalize(&source.source_path)
                .map(|path| (path.to_string_lossy().into_owned(), parent))
                .map_err(StructureAnalysisError::Io)
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let mut helpers = Vec::new();
    for (index, member) in plan.members.iter().enumerate() {
        let source_path = member_source_path(&member.origin)?;
        let parent = parent_by_path.get(&source_path).ok_or_else(|| {
            StructureAnalysisError::Graph("MD RAID member has no registered parent".into())
        })?;
        let parent_node = source_node_id(&parent.source_id, parent.revision);
        graph.mappings.push(AddressMapping {
            id: mapping_id(&version.id.0, index),
            source_node: parent_node.clone(),
            target_node: version_node.clone(),
            input_space: AddressSpace::PhysicalByte,
            output_space: AddressSpace::LogicalBlock,
            unit: "byte".into(),
            input_range: None,
            output_range: None,
            mapping_sha256: version.mapping_sha256.clone(),
            adapter_version: version.adapter_version.clone(),
        });
        helpers.push(RecoveryHelper {
            node: parent_node,
            role: "member".into(),
            slot: member.superblock.slot_index().unwrap_or(index as u32),
            required: true,
            range: None,
        });
    }
    if let Some(required) = required_helpers(plan.level, plan.raid_disks) {
        if helpers.len() >= required as usize {
            graph.recovery_relations.push(RecoveryRelation {
                id: format!("recovery:{}", version.id.0),
                target_node: version_node.clone(),
                groups: vec![RecoveryGroup {
                    id: format!("group:{}", version.id.0),
                    helpers,
                    required_helpers: required,
                    algorithm_id: "mdraid-native".into(),
                    algorithm_version: version.adapter_version.clone(),
                    effective_ranges: vec![ByteRange::new(0, logical_size)
                        .map_err(|error| StructureAnalysisError::Graph(error.to_string()))?],
                }],
                verification: version.mapping_sha256.clone(),
            });
        }
    }
    Ok(())
}

fn member_source_path(
    origin: &evidence_access::mdraid::MdRaidMemberOrigin,
) -> Result<String, StructureAnalysisError> {
    match origin {
        evidence_access::mdraid::MdRaidMemberOrigin::Direct { source_path, .. } => {
            Ok(std::fs::canonicalize(source_path)?
                .to_string_lossy()
                .into_owned())
        }
        evidence_access::mdraid::MdRaidMemberOrigin::Lvm { identity, .. } => identity
            .pv_sources
            .first()
            .map(|source| {
                std::fs::canonicalize(&source.source_path)
                    .map(|path| path.to_string_lossy().into_owned())
                    .map_err(StructureAnalysisError::Io)
            })
            .transpose()?
            .ok_or_else(|| {
                StructureAnalysisError::Graph("MD RAID LVM member has no PV source".into())
            }),
    }
}

pub(super) fn required_helpers(level: i32, raid_disks: u32) -> Option<u32> {
    match level {
        -1 | 0 => Some(raid_disks),
        1 => Some(1),
        4 | 5 => raid_disks.checked_sub(1),
        6 => raid_disks.checked_sub(2),
        10 => raid_disks.checked_div(2),
        _ => None,
    }
}

fn add_edge(
    graph: &mut StructureGraph,
    from: StructureNodeId,
    to: StructureNodeId,
    kind: StructureRelationKind,
    role: &str,
) {
    graph.edges.push(StructureEdge {
        from,
        to,
        kind,
        role: role.into(),
        required: true,
    });
}

pub(crate) fn source_node_id(source_id: &str, revision: u64) -> StructureNodeId {
    StructureNodeId(format!("source:{source_id}@{revision}"))
}

fn mapping_id(version_id: &str, member_index: usize) -> String {
    let digest = Sha256::digest(format!("{version_id}:{member_index}").as_bytes());
    format!("mapping:{}", hex::encode(digest))
}
