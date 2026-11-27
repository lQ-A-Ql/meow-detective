use domain::structure::{
    ByteRangeSet, RecoveryGroup, RecoveryHelper, RecoveryRelation, StructureEdge, StructureGraph,
    StructureGraphError, StructureNode, StructureNodeId, StructureNodeKind, StructureRelationKind,
};

#[test]
fn content_dag_rejects_cycles_while_control_edges_may_cycle() {
    let a = node("a");
    let b = node("b");
    let mut graph = StructureGraph {
        nodes: vec![a.clone(), b.clone()],
        edges: vec![
            edge(&a.id, &b.id, StructureRelationKind::ContentDependency),
            edge(&b.id, &a.id, StructureRelationKind::ContentDependency),
        ],
        mappings: Vec::new(),
        recovery_relations: Vec::new(),
    };
    assert!(matches!(
        graph.validate(),
        Err(StructureGraphError::DependencyCycle)
    ));
    graph.edges = vec![
        edge(&a.id, &b.id, StructureRelationKind::DeploymentControl),
        edge(&b.id, &a.id, StructureRelationKind::DeploymentControl),
    ];
    graph.validate().unwrap();
    assert_eq!(graph.content_dependency_order().unwrap().len(), 2);
}

#[test]
fn recovery_helpers_must_exist_and_meet_the_required_count() {
    let target = node("target");
    let helper = node("helper");
    let mut graph = StructureGraph {
        nodes: vec![target.clone(), helper.clone()],
        edges: Vec::new(),
        mappings: Vec::new(),
        recovery_relations: vec![RecoveryRelation {
            id: "recovery".into(),
            target_node: target.id.clone(),
            groups: vec![RecoveryGroup {
                id: "group".into(),
                helpers: vec![RecoveryHelper {
                    node: helper.id.clone(),
                    role: "member".into(),
                    slot: 0,
                    required: true,
                    range: None,
                }],
                required_helpers: 2,
                algorithm_id: "raid".into(),
                algorithm_version: "1".into(),
                effective_ranges: Vec::new(),
            }],
            verification: "verified".into(),
        }],
    };
    assert!(matches!(
        graph.validate(),
        Err(StructureGraphError::InvalidRecoveryGroup(_))
    ));
    graph.recovery_relations[0].groups[0].required_helpers = 1;
    graph.validate().unwrap();
    graph.recovery_relations[0].groups[0].helpers[0].node = StructureNodeId("missing".into());
    assert!(matches!(
        graph.validate(),
        Err(StructureGraphError::MissingNode(_))
    ));
}

#[test]
fn coverage_ranges_remain_separate_from_graph_edges() {
    let range = ByteRangeSet::new(
        4096,
        [
            domain::structure::ByteRange::new(0, 1024).unwrap(),
            domain::structure::ByteRange::new(2048, 1024).unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(range.ranges().len(), 2);
    assert_eq!(range.covered_bytes(), 2048);
    assert!(!range.is_full());
}

fn node(name: &str) -> StructureNode {
    StructureNode {
        id: StructureNodeId(name.into()),
        kind: StructureNodeKind::ObjectVersion,
        label: name.into(),
        details: serde_json::Value::Null,
    }
}

fn edge(
    from: &StructureNodeId,
    to: &StructureNodeId,
    kind: StructureRelationKind,
) -> StructureEdge {
    StructureEdge {
        from: from.clone(),
        to: to.clone(),
        kind,
        role: "test".into(),
        required: true,
    }
}
