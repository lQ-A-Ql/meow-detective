use domain::{
    structure::{
        ByteRange, ByteRangeSet, ReadDemand, ReadIntent, ReadPriority, ReadVerification,
        StorageObjectVersionKind, StructureAnalysisPlan, StructureStage, StructureWorkItem,
    },
    DataSourceKind,
};
use persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo;

use super::{
    adapter::{
        AdapterRegistry, FILESYSTEM_ADAPTER_ID, FILESYSTEM_ADAPTER_VERSION, LVM_ADAPTER_ID,
        MDRAID_ADAPTER_ID, RBD_ADAPTER_ID, REGISTERED_SOURCE_ADAPTER_ID,
    },
    graph_builder::{build_structure_graph, SourceContext},
    request::{StructureAnalysisError, StructureAnalysisRequest},
    scheduler::schedule_work_items,
};

mod file_targets;

pub fn plan_structure_analysis(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    request: &StructureAnalysisRequest,
) -> Result<StructureAnalysisPlan, StructureAnalysisError> {
    if request.consumers.is_empty() {
        return Err(StructureAnalysisError::Plan(
            "structure analysis consumers are missing".into(),
        ));
    }
    let build = build_structure_graph(conn, case_root, &request.case_id, &request.source_ids)?;
    let revisions = SourceRevisionRepo::new(conn);
    for source in &build.sources {
        if source
            .versions
            .iter()
            .any(|version| version.version_kind == StorageObjectVersionKind::DerivedOutput)
        {
            revisions.require_current_inputs(&request.case_id.0, &source.data_source.id.0)?;
        }
    }
    let registry = AdapterRegistry::default_registry()?;
    let mut work_items = Vec::new();
    for source in &build.sources {
        if matches!(
            source.data_source.kind,
            DataSourceKind::CephFs | DataSourceKind::BtrfsPool
        ) {
            file_targets::append(conn, case_root, source, request, &registry, &mut work_items)?;
            continue;
        }
        let md_item = add_source_work_item(source, request, &registry, &mut work_items)?;
        add_filesystem_work_items(
            source,
            request,
            &registry,
            md_item.as_deref(),
            &mut work_items,
        )?;
    }
    let work_items = schedule_work_items(work_items)?;
    let plan = StructureAnalysisPlan {
        graph: build.graph,
        adapters: registry.descriptors(),
        work_items,
        stages: StructureStage::all(),
    };
    plan.validate()
        .map_err(|error| StructureAnalysisError::Plan(error.to_string()))?;
    Ok(plan)
}

fn add_source_work_item(
    source: &SourceContext,
    request: &StructureAnalysisRequest,
    registry: &AdapterRegistry,
    work_items: &mut Vec<StructureWorkItem>,
) -> Result<Option<String>, StructureAnalysisError> {
    let adapter_id = match source.data_source.kind {
        DataSourceKind::MdRaid => MDRAID_ADAPTER_ID,
        DataSourceKind::LvmLogicalVolume => LVM_ADAPTER_ID,
        DataSourceKind::CephRbd => RBD_ADAPTER_ID,
        DataSourceKind::Raw | DataSourceKind::E01 => REGISTERED_SOURCE_ADAPTER_ID,
        _ if request.intent == ReadIntent::ContentRead => {
            return Err(StructureAnalysisError::UnsupportedAdapter(
                source.data_source.kind.to_string(),
            ));
        }
        _ => return Ok(None),
    };
    if source.revision.logical_size == 0 {
        return Err(StructureAnalysisError::EmptySource(
            source.data_source.id.0.clone(),
        ));
    }
    let adapter = registry
        .get(adapter_id)
        .ok_or_else(|| StructureAnalysisError::UnsupportedAdapter(adapter_id.into()))?;
    require_features(adapter, &request.required_features)?;
    let id = format!(
        "{adapter_id}-metadata:{}:{}",
        source.data_source.id.0, source.revision_ref.revision
    );
    let version = source
        .versions
        .iter()
        .find(|version| version.version_kind == StorageObjectVersionKind::DerivedOutput);
    if !matches!(
        source.data_source.kind,
        DataSourceKind::Raw | DataSourceKind::E01
    ) && version.is_none()
    {
        return Err(StructureAnalysisError::Plan(
            "derived source has no public object version".into(),
        ));
    }
    let fingerprint = version
        .map(|version| version.mapping_sha256.clone())
        .unwrap_or_else(|| {
            format!(
                "source-revision:{}:{}",
                source.data_source.id.0, source.revision_ref.revision
            )
        });
    let length = if request.intent == ReadIntent::ContentRead {
        source.revision.logical_size
    } else {
        source.revision.logical_size.min(4096)
    };
    let demand = demand(
        source,
        &fingerprint,
        ByteRangeSet::new(
            source.revision.logical_size,
            [ByteRange::new(0, length)
                .map_err(|error| StructureAnalysisError::Plan(error.to_string()))?],
        )
        .map_err(|error| StructureAnalysisError::Plan(error.to_string()))?,
        request.intent,
        request.priority,
        request.consumers.clone(),
        if request.intent == ReadIntent::ContentRead {
            ReadVerification::ContentDigest
        } else {
            ReadVerification::RangeCoverage
        },
        request.attempt.clone(),
    )?;
    work_items.push(StructureWorkItem {
        id: id.clone(),
        source_revision: source.revision_ref.clone(),
        target_fingerprint: fingerprint,
        adapter_id: adapter_id.into(),
        adapter_version: adapter.version.clone(),
        ranges: demand.ranges.clone(),
        intent: request.intent,
        priority: request.priority,
        dependencies: Vec::new(),
        consumers: request.consumers.clone(),
        demand: Some(demand),
    });
    Ok(Some(id))
}

fn add_filesystem_work_items(
    source: &SourceContext,
    request: &StructureAnalysisRequest,
    registry: &AdapterRegistry,
    md_item: Option<&str>,
    work_items: &mut Vec<StructureWorkItem>,
) -> Result<(), StructureAnalysisError> {
    if !matches!(
        source.data_source.kind,
        DataSourceKind::MdRaid | DataSourceKind::LvmLogicalVolume | DataSourceKind::CephRbd
    ) {
        return Ok(());
    }
    let adapter = registry
        .get(FILESYSTEM_ADAPTER_ID)
        .ok_or_else(|| StructureAnalysisError::UnsupportedAdapter(FILESYSTEM_ADAPTER_ID.into()))?;
    require_features(adapter, &request.required_features)?;
    if adapter.version != FILESYSTEM_ADAPTER_VERSION {
        return Err(StructureAnalysisError::UnsupportedAdapter(
            FILESYSTEM_ADAPTER_ID.into(),
        ));
    }
    for partition in source
        .partitions
        .iter()
        .filter(|partition| partition.filesystem.is_some())
    {
        let range = ByteRange::new(partition.offset, partition.length.min(4096))
            .map_err(|error| StructureAnalysisError::Plan(error.to_string()))?;
        let ranges = ByteRangeSet::new(source.revision.logical_size, [range])
            .map_err(|error| StructureAnalysisError::Plan(error.to_string()))?;
        let fingerprint = format!(
            "filesystem-window-v1:{}:{}:{}",
            source.data_source.id.0,
            partition.partition_index,
            partition.filesystem.as_deref().unwrap_or("unknown")
        );
        let demand = demand(
            source,
            &fingerprint,
            ranges,
            ReadIntent::MetadataInspection,
            request.priority,
            request.consumers.clone(),
            ReadVerification::RangeCoverage,
            request.attempt.clone(),
        )?;
        work_items.push(StructureWorkItem {
            id: format!(
                "filesystem-window:{}:{}",
                source.data_source.id.0, partition.partition_index
            ),
            source_revision: source.revision_ref.clone(),
            target_fingerprint: fingerprint,
            adapter_id: FILESYSTEM_ADAPTER_ID.into(),
            adapter_version: adapter.version.clone(),
            ranges: demand.ranges.clone(),
            intent: ReadIntent::MetadataInspection,
            priority: request.priority,
            dependencies: md_item.into_iter().map(str::to_owned).collect(),
            consumers: request.consumers.clone(),
            demand: Some(demand),
        });
    }
    Ok(())
}

fn require_features(
    adapter: &domain::structure::StructureAdapterDescriptor,
    required: &std::collections::BTreeSet<String>,
) -> Result<(), StructureAnalysisError> {
    for feature in required {
        if !adapter.features.contains(feature) {
            return Err(StructureAnalysisError::UnsupportedFeature {
                adapter: adapter.id.clone(),
                feature: feature.clone(),
            });
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn demand(
    source: &SourceContext,
    target_fingerprint: &str,
    ranges: ByteRangeSet,
    intent: ReadIntent,
    priority: ReadPriority,
    consumers: Vec<String>,
    verification: ReadVerification,
    attempt: Option<domain::structure::StructureAttemptRef>,
) -> Result<ReadDemand, StructureAnalysisError> {
    let demand = ReadDemand::new(
        source.revision_ref.clone(),
        target_fingerprint.into(),
        intent,
        ranges,
        priority,
        None,
        true,
        consumers,
        verification,
    )
    .map_err(|error| StructureAnalysisError::Plan(error.to_string()))?;
    match attempt {
        Some(attempt) => demand
            .with_attempt(attempt)
            .map_err(|error| StructureAnalysisError::Plan(error.to_string())),
        None => Ok(demand),
    }
}
