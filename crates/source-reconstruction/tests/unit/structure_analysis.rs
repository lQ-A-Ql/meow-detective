use domain::structure::{
    ByteRange, ByteRangeSet, ReadIntent, ReadPriority, SourceRevisionRef, StructureWorkItem,
};

use crate::structure_analysis::{graph_builder::required_helpers, schedule_work_items};

#[test]
fn scheduler_merges_exact_work_duplicates_and_orders_priority() {
    let revision = SourceRevisionRef {
        case_id: "case".into(),
        source_id: "source".into(),
        revision: 1,
    };
    let first = work_item(
        "interactive",
        revision.clone(),
        ReadPriority::Interactive,
        [0, 4096],
        "preview",
    );
    let duplicate = work_item(
        "interactive",
        revision.clone(),
        ReadPriority::Interactive,
        [0, 4096],
        "verification",
    );
    let deferred = work_item(
        "deferred",
        revision,
        ReadPriority::Deferred,
        [0, 1024],
        "scan",
    );
    let scheduled = schedule_work_items(vec![deferred, first, duplicate]).unwrap();
    assert_eq!(scheduled.len(), 2);
    assert_eq!(scheduled[0].id, "interactive");
    assert_eq!(scheduled[0].ranges.covered_bytes(), 4096);
    assert_eq!(
        scheduled[0].consumers,
        vec!["preview".to_owned(), "verification".to_owned()]
    );
    assert_eq!(scheduled[1].priority, ReadPriority::Deferred);
}

#[test]
fn recovery_capacity_separates_linear_mirror_and_parity_layouts() {
    assert_eq!(required_helpers(-1, 2), Some(2));
    assert_eq!(required_helpers(0, 2), Some(2));
    assert_eq!(required_helpers(1, 2), Some(1));
    assert_eq!(required_helpers(5, 3), Some(2));
    assert_eq!(required_helpers(6, 4), Some(2));
    assert_eq!(required_helpers(10, 4), Some(2));
    assert_eq!(required_helpers(99, 4), None);
}

#[test]
fn scheduler_keeps_dependency_order_and_rejects_missing_or_cyclic_inputs() {
    let source = SourceRevisionRef {
        case_id: "case".into(),
        source_id: "source".into(),
        revision: 1,
    };
    let parent = work_item(
        "parent",
        source.clone(),
        ReadPriority::Deferred,
        [0, 100],
        "metadata",
    );
    let mut child = work_item(
        "child",
        source,
        ReadPriority::Interactive,
        [100, 100],
        "preview",
    );
    child.dependencies.push(parent.id.clone());
    let scheduled = schedule_work_items(vec![child.clone(), parent.clone()]).unwrap();
    assert_eq!(
        scheduled
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        ["parent", "child"]
    );
    assert!(schedule_work_items(vec![child.clone()]).is_err());
    let mut cyclic = parent;
    cyclic.dependencies.push(child.id.clone());
    assert!(schedule_work_items(vec![child, cyclic]).is_err());
}

#[test]
fn scheduler_preserves_verification_owner_deadline_and_read_order() {
    let source = SourceRevisionRef {
        case_id: "case".into(),
        source_id: "source".into(),
        revision: 1,
    };
    let mut original = work_item(
        "original",
        source.clone(),
        ReadPriority::Deferred,
        [0, 100],
        "catalog",
    );
    let mut duplicate = work_item(
        "duplicate",
        source.clone(),
        ReadPriority::Interactive,
        [0, 100],
        "preview",
    );
    for (item, deadline, reorderable) in [(&mut original, 20, true), (&mut duplicate, 10, false)] {
        item.demand = Some(
            domain::structure::ReadDemand::new(
                source.clone(),
                "target".into(),
                item.intent,
                item.ranges.clone(),
                item.priority,
                Some(deadline),
                reorderable,
                item.consumers.clone(),
                domain::structure::ReadVerification::ContentDigest,
            )
            .unwrap()
            .with_attempt(domain::structure::StructureAttemptRef {
                attempt_id: "attempt".into(),
                owner: "owner".into(),
            })
            .unwrap(),
        );
    }
    let scheduled = schedule_work_items(vec![original, duplicate]).unwrap();
    assert_eq!(scheduled.len(), 1);
    let demand = scheduled[0].demand.as_ref().unwrap();
    assert_eq!(demand.deadline_unix_ms, Some(10));
    assert!(!demand.reorderable);
    assert_eq!(
        demand.verification,
        domain::structure::ReadVerification::ContentDigest
    );
    assert_eq!(demand.attempt.as_ref().unwrap().owner, "owner");
    let mut another_owner = scheduled[0].clone();
    another_owner.id = "another-owner".into();
    another_owner.demand = Some(
        demand
            .clone()
            .with_attempt(domain::structure::StructureAttemptRef {
                attempt_id: "attempt".into(),
                owner: "new-owner".into(),
            })
            .unwrap(),
    );
    assert_eq!(
        schedule_work_items(vec![scheduled[0].clone(), another_owner])
            .unwrap()
            .len(),
        2
    );
}

fn work_item(
    id: &str,
    revision: SourceRevisionRef,
    priority: ReadPriority,
    range: [u64; 2],
    consumer: &str,
) -> StructureWorkItem {
    StructureWorkItem {
        id: id.into(),
        source_revision: revision,
        target_fingerprint: "target".into(),
        adapter_id: "adapter".into(),
        adapter_version: "1".into(),
        ranges: ByteRangeSet::new(8192, [ByteRange::new(range[0], range[1]).unwrap()]).unwrap(),
        intent: ReadIntent::ContentRead,
        priority,
        dependencies: Vec::new(),
        consumers: vec![consumer.into()],
        demand: None,
    }
}
