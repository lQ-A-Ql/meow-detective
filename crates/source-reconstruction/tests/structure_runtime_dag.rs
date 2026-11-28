use std::collections::{BTreeMap, BTreeSet};

use domain::structure::{
    ActivityState, ByteCoverage, ByteRange, CapabilityKind, OrderingRequirement, StructureStage,
    TargetCoverage, TargetSelector, WorkIdentity,
};
use source_reconstruction::structure_runtime::{
    ActivityDag, ActivityDefinition, Readiness, RuntimeError,
};

fn definition(name: &str) -> ActivityDefinition {
    let range = ByteRange::new(0, 512).unwrap();
    let target = TargetSelector::Block {
        view_id: name.into(),
        range,
    };
    let selector = target
        .identity()
        .unwrap()
        .strip_prefix("target-selector-v1-")
        .unwrap()
        .to_owned();
    ActivityDefinition {
        work: WorkIdentity {
            operation_id: name.into(),
            adapter_id: "native-reader".into(),
            adapter_version: "1".into(),
            goal_id: "range-read".into(),
            input_capability_versions: BTreeSet::from(["input-v1".into()]),
            selector_sha256: selector,
            version_witness_sha256: "a".repeat(64),
            scope_witness_sha256: "b".repeat(64),
            ordering: OrderingRequirement::Unordered,
            policy_version: "1".into(),
        },
        stage: StructureStage::ReadVerification,
        target,
        scope_id: "scope".into(),
        required_capabilities: BTreeSet::from([CapabilityKind::BlockRange]),
        requested_coverage: TargetCoverage::Bytes(ByteCoverage {
            target_size: 512,
            requested: vec![range],
            unknown: vec![range],
            verified: vec![],
            unreadable: vec![],
            native_holes: vec![],
        }),
        dependencies: BTreeSet::new(),
        parameters: serde_json::Value::Null,
    }
}

#[test]
fn cycles_missing_dependencies_and_conflicting_definitions_leave_the_graph_unchanged() {
    let mut dag = ActivityDag::new(8, 16).unwrap();
    let mut first = definition("first");
    let mut second = definition("second");
    first.dependencies.insert(second.key().unwrap());
    second.dependencies.insert(first.key().unwrap());
    assert!(matches!(
        dag.insert_batch(vec![first, second]),
        Err(RuntimeError::DependencyCycle)
    ));
    assert_eq!(dag.definitions().count(), 0);
    assert_eq!((dag.node_count(), dag.edge_count()), (0, 0));
    let mut absent = definition("absent");
    absent.dependencies.insert("missing".into());
    assert!(matches!(
        dag.insert_batch(vec![absent]),
        Err(RuntimeError::MissingDependency)
    ));
    assert_eq!((dag.node_count(), dag.edge_count()), (0, 0));
    let valid = definition("valid");
    assert_eq!(
        dag.insert_batch(vec![valid.clone(), valid.clone()])
            .unwrap()
            .len(),
        1
    );
    let mut changed = valid.clone();
    changed.parameters = serde_json::json!({"mapping": "changed"});
    assert!(matches!(
        dag.insert_batch(vec![changed]),
        Err(RuntimeError::ConflictingWork)
    ));
    assert_eq!(dag.get(&valid.key().unwrap()), Some(&valid));
    assert_eq!((dag.node_count(), dag.edge_count()), (1, 0));
}

#[test]
fn failed_parent_blocks_only_its_required_successors() {
    let parent = definition("parent");
    let mut child = definition("child");
    let mut grandchild = definition("grandchild");
    let independent = definition("independent");
    child.dependencies.insert(parent.key().unwrap());
    grandchild.dependencies.insert(child.key().unwrap());
    let mut dag = ActivityDag::new(8, 16).unwrap();
    dag.insert_batch(vec![
        parent.clone(),
        child.clone(),
        grandchild.clone(),
        independent.clone(),
    ])
    .unwrap();
    let mut states = dag
        .definitions()
        .map(|(key, _)| (key.clone(), ActivityState::Pending))
        .collect::<BTreeMap<_, _>>();
    states.insert(parent.key().unwrap(), ActivityState::Failed);
    let keys = states
        .keys()
        .filter(|key| **key != parent.key().unwrap())
        .cloned()
        .collect();
    let mut readiness = Readiness::default();
    readiness.register(&keys, &dag, &states).unwrap();
    assert_eq!(readiness.pop_blocked(), Some(child.key().unwrap()));
    assert_eq!(readiness.pop_ready(), Some(independent.key().unwrap()));
    readiness.completed(&child.key().unwrap(), ActivityState::Blocked);
    assert_eq!(readiness.pop_blocked(), Some(grandchild.key().unwrap()));
}

#[test]
fn graph_limits_fail_before_adding_a_partial_batch() {
    let mut dag = ActivityDag::new(1, 1).unwrap();
    assert!(matches!(
        dag.insert_batch(vec![definition("a"), definition("b")]),
        Err(RuntimeError::Capacity)
    ));
    assert_eq!(dag.definitions().count(), 0);
    assert_eq!((dag.node_count(), dag.edge_count()), (0, 0));
    assert!(ActivityDag::new(0, 1).is_err());
    assert!(ActivityDag::new(1, 0).is_err());
}

#[test]
fn batches_resolve_existing_and_new_dependencies_independently_of_input_order() {
    let root = definition("root");
    let mut child = definition("child");
    child.dependencies.insert(root.key().unwrap());
    let mut leaf = definition("leaf");
    leaf.dependencies.insert(root.key().unwrap());
    leaf.dependencies.insert(child.key().unwrap());
    let mut dag = ActivityDag::new(4, 8).unwrap();
    assert_eq!(
        dag.insert_batch(vec![root.clone()]).unwrap(),
        BTreeSet::from([root.key().unwrap()])
    );
    assert_eq!((dag.node_count(), dag.edge_count()), (1, 0));
    assert_eq!(
        dag.insert_batch(vec![
            leaf.clone(),
            root.clone(),
            child.clone(),
            leaf.clone()
        ])
        .unwrap(),
        BTreeSet::from([child.key().unwrap(), leaf.key().unwrap()])
    );
    assert_eq!((dag.node_count(), dag.edge_count()), (3, 3));
    let mut descendant = definition("descendant");
    descendant.dependencies.insert(leaf.key().unwrap());
    assert_eq!(
        dag.insert_batch(vec![descendant.clone()]).unwrap(),
        BTreeSet::from([descendant.key().unwrap()])
    );
    assert_eq!((dag.node_count(), dag.edge_count()), (4, 4));
    for activity in [&root, &child, &leaf, &descendant] {
        assert_eq!(dag.get(&activity.key().unwrap()), Some(activity));
    }
}

#[test]
fn idempotent_batches_preserve_nodes_and_edges_at_capacity() {
    let root = definition("root");
    let mut child = definition("child");
    child.dependencies.insert(root.key().unwrap());
    let mut leaf = definition("leaf");
    leaf.dependencies.insert(root.key().unwrap());
    leaf.dependencies.insert(child.key().unwrap());
    let mut dag = ActivityDag::new(3, 3).unwrap();
    dag.insert_batch(vec![root.clone(), child.clone(), leaf.clone()])
        .unwrap();
    for _ in 0..3 {
        assert!(dag
            .insert_batch(vec![
                leaf.clone(),
                root.clone(),
                child.clone(),
                leaf.clone()
            ])
            .unwrap()
            .is_empty());
        assert_eq!(dag.definitions().count(), 3);
        assert_eq!((dag.node_count(), dag.edge_count()), (3, 3));
        for activity in [&root, &child, &leaf] {
            assert_eq!(dag.get(&activity.key().unwrap()), Some(activity));
        }
    }
    assert!(dag.insert_batch(Vec::new()).unwrap().is_empty());
    assert_eq!((dag.node_count(), dag.edge_count()), (3, 3));
}

#[test]
fn cyclic_batch_preserves_existing_graph_and_the_same_work_can_be_retried() {
    let root = definition("root");
    let mut child = definition("child");
    child.dependencies.insert(root.key().unwrap());
    let mut dag = ActivityDag::new(4, 8).unwrap();
    dag.insert_batch(vec![root.clone(), child.clone()]).unwrap();
    let mut first = definition("first");
    let mut second = definition("second");
    first.dependencies.insert(child.key().unwrap());
    first.dependencies.insert(second.key().unwrap());
    second.dependencies.insert(first.key().unwrap());
    assert!(matches!(
        dag.insert_batch(vec![first.clone(), second.clone()]),
        Err(RuntimeError::DependencyCycle)
    ));
    assert_eq!(dag.definitions().count(), 2);
    assert_eq!((dag.node_count(), dag.edge_count()), (2, 1));
    assert_eq!(dag.get(&root.key().unwrap()), Some(&root));
    assert_eq!(dag.get(&child.key().unwrap()), Some(&child));
    assert!(dag.get(&first.key().unwrap()).is_none());
    assert!(dag.get(&second.key().unwrap()).is_none());
    first.dependencies.remove(&second.key().unwrap());
    assert_eq!(
        dag.insert_batch(vec![second.clone(), first.clone()])
            .unwrap(),
        BTreeSet::from([first.key().unwrap(), second.key().unwrap()])
    );
    assert_eq!((dag.node_count(), dag.edge_count()), (4, 3));
    assert_eq!(dag.get(&first.key().unwrap()), Some(&first));
    assert_eq!(dag.get(&second.key().unwrap()), Some(&second));
}

#[test]
fn missing_or_invalid_work_rejects_the_batch_without_committing_valid_siblings() {
    let root = definition("root");
    let mut child = definition("child");
    child.dependencies.insert(root.key().unwrap());
    let mut absent = definition("absent");
    absent.dependencies.insert("missing".into());
    let mut dag = ActivityDag::new(4, 8).unwrap();
    dag.insert_batch(vec![root.clone()]).unwrap();
    assert!(matches!(
        dag.insert_batch(vec![child.clone(), absent.clone()]),
        Err(RuntimeError::MissingDependency)
    ));
    assert_eq!((dag.node_count(), dag.edge_count()), (1, 0));
    assert_eq!(dag.definitions().count(), 1);
    assert!(dag.get(&child.key().unwrap()).is_none());
    assert!(dag.get(&absent.key().unwrap()).is_none());
    let mut invalid = definition("invalid");
    invalid.scope_id.clear();
    assert!(matches!(
        dag.insert_batch(vec![child.clone(), invalid]),
        Err(RuntimeError::InvalidDefinition)
    ));
    assert_eq!((dag.node_count(), dag.edge_count()), (1, 0));
    assert_eq!(dag.definitions().count(), 1);
    assert_eq!(dag.get(&root.key().unwrap()), Some(&root));
    assert!(dag.get(&child.key().unwrap()).is_none());
    dag.insert_batch(vec![child.clone()]).unwrap();
    assert_eq!((dag.node_count(), dag.edge_count()), (2, 1));
    assert_eq!(dag.get(&child.key().unwrap()), Some(&child));
}

#[test]
fn conflicting_new_work_preserves_existing_graph_and_accepts_a_valid_retry() {
    let root = definition("root");
    let mut child = definition("child");
    child.dependencies.insert(root.key().unwrap());
    let mut changed = child.clone();
    changed.parameters = serde_json::json!({"mapping": "changed"});
    let mut dag = ActivityDag::new(3, 4).unwrap();
    dag.insert_batch(vec![root.clone()]).unwrap();
    assert!(matches!(
        dag.insert_batch(vec![child.clone(), changed]),
        Err(RuntimeError::ConflictingWork)
    ));
    assert_eq!(dag.definitions().count(), 1);
    assert_eq!((dag.node_count(), dag.edge_count()), (1, 0));
    assert_eq!(dag.get(&root.key().unwrap()), Some(&root));
    assert!(dag.get(&child.key().unwrap()).is_none());
    dag.insert_batch(vec![child.clone()]).unwrap();
    assert_eq!((dag.node_count(), dag.edge_count()), (2, 1));
    assert_eq!(dag.get(&child.key().unwrap()), Some(&child));
}

#[test]
fn conflict_with_committed_work_does_not_commit_a_new_sibling() {
    let root = definition("root");
    let mut child = definition("child");
    child.dependencies.insert(root.key().unwrap());
    let mut leaf = definition("leaf");
    leaf.dependencies.insert(child.key().unwrap());
    let mut changed = child.clone();
    changed.dependencies.insert(leaf.key().unwrap());
    let mut dag = ActivityDag::new(3, 4).unwrap();
    dag.insert_batch(vec![root.clone(), child.clone()]).unwrap();
    assert!(matches!(
        dag.insert_batch(vec![leaf.clone(), changed]),
        Err(RuntimeError::ConflictingWork)
    ));
    assert_eq!(dag.definitions().count(), 2);
    assert_eq!((dag.node_count(), dag.edge_count()), (2, 1));
    assert_eq!(dag.get(&root.key().unwrap()), Some(&root));
    assert_eq!(dag.get(&child.key().unwrap()), Some(&child));
    assert!(dag.get(&leaf.key().unwrap()).is_none());
    dag.insert_batch(vec![leaf.clone()]).unwrap();
    assert_eq!((dag.node_count(), dag.edge_count()), (3, 2));
    assert_eq!(dag.get(&leaf.key().unwrap()), Some(&leaf));
}

#[test]
fn self_dependency_failure_preserves_existing_graph_and_accepts_a_valid_retry() {
    let root = definition("root");
    let mut child = definition("child");
    child.dependencies.insert(root.key().unwrap());
    let child_key = child.key().unwrap();
    child.dependencies.insert(child_key.clone());
    let mut dag = ActivityDag::new(2, 2).unwrap();
    dag.insert_batch(vec![root.clone()]).unwrap();
    assert!(matches!(
        dag.insert_batch(vec![child.clone()]),
        Err(RuntimeError::DependencyCycle)
    ));
    assert_eq!(dag.definitions().count(), 1);
    assert_eq!((dag.node_count(), dag.edge_count()), (1, 0));
    assert_eq!(dag.get(&root.key().unwrap()), Some(&root));
    assert!(dag.get(&child_key).is_none());
    child.dependencies.remove(&child_key);
    dag.insert_batch(vec![child.clone()]).unwrap();
    assert_eq!((dag.node_count(), dag.edge_count()), (2, 1));
    assert_eq!(dag.get(&child_key), Some(&child));
}

#[test]
fn node_capacity_failure_preserves_existing_graph_and_accepts_a_smaller_batch() {
    let root = definition("root");
    let mut child = definition("child");
    child.dependencies.insert(root.key().unwrap());
    let leaf = definition("leaf");
    let mut dag = ActivityDag::new(2, 4).unwrap();
    dag.insert_batch(vec![root.clone()]).unwrap();
    assert!(matches!(
        dag.insert_batch(vec![child.clone(), leaf.clone()]),
        Err(RuntimeError::Capacity)
    ));
    assert_eq!(dag.definitions().count(), 1);
    assert_eq!((dag.node_count(), dag.edge_count()), (1, 0));
    assert_eq!(dag.get(&root.key().unwrap()), Some(&root));
    assert!(dag.get(&child.key().unwrap()).is_none());
    assert!(dag.get(&leaf.key().unwrap()).is_none());
    dag.insert_batch(vec![child.clone()]).unwrap();
    assert_eq!((dag.node_count(), dag.edge_count()), (2, 1));
}

#[test]
fn edge_capacity_failure_preserves_existing_graph_and_accepts_zero_edge_work() {
    let root = definition("root");
    let mut child = definition("child");
    child.dependencies.insert(root.key().unwrap());
    let mut first = definition("first");
    first.dependencies.insert(child.key().unwrap());
    let mut second = definition("second");
    second.dependencies.insert(first.key().unwrap());
    let mut dag = ActivityDag::new(4, 2).unwrap();
    dag.insert_batch(vec![root.clone(), child.clone()]).unwrap();
    assert!(matches!(
        dag.insert_batch(vec![first.clone(), second.clone()]),
        Err(RuntimeError::Capacity)
    ));
    assert_eq!(dag.definitions().count(), 2);
    assert_eq!((dag.node_count(), dag.edge_count()), (2, 1));
    assert_eq!(dag.get(&root.key().unwrap()), Some(&root));
    assert_eq!(dag.get(&child.key().unwrap()), Some(&child));
    assert!(dag.get(&first.key().unwrap()).is_none());
    assert!(dag.get(&second.key().unwrap()).is_none());
    dag.insert_batch(vec![first.clone()]).unwrap();
    assert_eq!((dag.node_count(), dag.edge_count()), (3, 2));
    assert!(dag.insert_batch(vec![first.clone()]).unwrap().is_empty());
    assert_eq!((dag.node_count(), dag.edge_count()), (3, 2));
    let independent = definition("independent");
    dag.insert_batch(vec![independent.clone()]).unwrap();
    assert_eq!((dag.node_count(), dag.edge_count()), (4, 2));
    assert_eq!(dag.get(&first.key().unwrap()), Some(&first));
    assert_eq!(dag.get(&independent.key().unwrap()), Some(&independent));
}
