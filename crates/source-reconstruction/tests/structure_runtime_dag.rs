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
    let mut absent = definition("absent");
    absent.dependencies.insert("missing".into());
    assert!(matches!(
        dag.insert_batch(vec![absent]),
        Err(RuntimeError::MissingDependency)
    ));
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
    assert!(ActivityDag::new(0, 1).is_err());
}
