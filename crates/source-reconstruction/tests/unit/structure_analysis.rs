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
