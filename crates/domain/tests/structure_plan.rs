use std::collections::BTreeSet;

use domain::structure::{
    AdapterOperation, AddressSpace, ByteRange, ByteRangeSet, ReadDemand, ReadIntent, ReadPriority,
    ReadVerification, SourceRevisionRef, StructureAdapterDescriptor, StructureAnalysisPlan,
    StructureGraph, StructurePlanError, StructureWorkItem,
};

#[test]
fn plan_rejects_unknown_adapter_version_and_missing_dependency() {
    let revision = SourceRevisionRef {
        case_id: "case".into(),
        source_id: "source".into(),
        revision: 1,
    };
    let work = StructureWorkItem {
        id: "work".into(),
        source_revision: revision.clone(),
        target_fingerprint: "target".into(),
        adapter_id: "adapter".into(),
        adapter_version: "2".into(),
        ranges: ByteRangeSet::new(4096, [ByteRange::new(0, 4096).unwrap()]).unwrap(),
        intent: ReadIntent::ContentRead,
        priority: ReadPriority::Interactive,
        dependencies: vec!["missing".into()],
        consumers: vec!["test".into()],
        demand: Some(
            ReadDemand::new(
                revision,
                "target".into(),
                ReadIntent::ContentRead,
                ByteRangeSet::new(4096, [ByteRange::new(0, 4096).unwrap()]).unwrap(),
                ReadPriority::Interactive,
                None,
                true,
                vec!["test".into()],
                ReadVerification::RangeCoverage,
            )
            .unwrap(),
        ),
    };
    let mut plan = StructureAnalysisPlan {
        graph: empty_graph(),
        adapters: vec![adapter("1")],
        work_items: vec![work],
    };
    assert!(matches!(
        plan.validate(),
        Err(StructurePlanError::AdapterVersionMismatch(_))
    ));
    plan.adapters[0].version = "2".into();
    assert!(matches!(
        plan.validate(),
        Err(StructurePlanError::MissingDependency(_))
    ));
}

#[test]
fn plan_requires_the_plan_operation_and_valid_ranges() {
    let adapter = StructureAdapterDescriptor {
        operations: [AdapterOperation::Probe]
            .into_iter()
            .collect::<BTreeSet<_>>(),
        ..adapter("1")
    };
    let mut plan = StructureAnalysisPlan {
        graph: empty_graph(),
        adapters: vec![adapter],
        work_items: Vec::new(),
    };
    plan.validate().unwrap();
    plan.work_items.push(StructureWorkItem {
        id: "work".into(),
        source_revision: SourceRevisionRef {
            case_id: "case".into(),
            source_id: "source".into(),
            revision: 1,
        },
        target_fingerprint: "target".into(),
        adapter_id: "adapter".into(),
        adapter_version: "1".into(),
        ranges: ByteRangeSet::new(4096, [ByteRange::new(0, 4096).unwrap()]).unwrap(),
        intent: ReadIntent::ContentRead,
        priority: ReadPriority::Normal,
        dependencies: Vec::new(),
        consumers: vec!["test".into()],
        demand: None,
    });
    assert!(matches!(
        plan.validate(),
        Err(StructurePlanError::UnsupportedOperation(_))
    ));
}

fn adapter(version: &str) -> StructureAdapterDescriptor {
    StructureAdapterDescriptor {
        id: "adapter".into(),
        version: version.into(),
        formats: vec!["format".into()],
        features: BTreeSet::new(),
        input_spaces: vec![AddressSpace::PhysicalByte],
        output_spaces: vec![AddressSpace::LogicalBlock],
        operations: [
            AdapterOperation::Plan,
            AdapterOperation::Probe,
            AdapterOperation::Inspect,
            AdapterOperation::Bind,
            AdapterOperation::SelectVersion,
            AdapterOperation::Verify,
            AdapterOperation::Project,
        ]
        .into_iter()
        .collect(),
        max_workers: 1,
        requires_independent_readers: true,
    }
}

fn empty_graph() -> StructureGraph {
    StructureGraph {
        nodes: Vec::new(),
        edges: Vec::new(),
        mappings: Vec::new(),
        recovery_relations: Vec::new(),
    }
}
