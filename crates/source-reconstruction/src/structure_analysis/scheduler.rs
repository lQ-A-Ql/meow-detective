use std::collections::{BTreeMap, BTreeSet};

use domain::structure::{ByteRangeSet, ReadDemand, StructureWorkItem};

use super::request::StructureAnalysisError;

pub const MAX_WORK_ITEMS: usize = 4096;

pub fn schedule_work_items(
    items: Vec<StructureWorkItem>,
) -> Result<Vec<StructureWorkItem>, StructureAnalysisError> {
    if items.len() > MAX_WORK_ITEMS {
        return Err(StructureAnalysisError::WorkItemLimit);
    }
    let mut grouped = BTreeMap::<String, StructureWorkItem>::new();
    let mut aliases = BTreeMap::<String, String>::new();
    for item in items {
        if let Some(demand) = &item.demand {
            if demand.source_revision != item.source_revision
                || demand.ranges != item.ranges
                || demand.target_fingerprint != item.target_fingerprint
                || demand.intent != item.intent
            {
                return Err(StructureAnalysisError::Plan(
                    "work item and read demand differ".into(),
                ));
            }
        }
        let key = item_key(&item);
        if let Some(existing) = grouped.get_mut(&key) {
            aliases.insert(item.id.clone(), existing.id.clone());
            merge(existing, item)?;
        } else {
            grouped.insert(key, item);
        }
    }
    let mut items = grouped.into_values().collect::<Vec<_>>();
    let ids = items
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    for item in &mut items {
        item.dependencies = item
            .dependencies
            .iter()
            .filter_map(|dependency| {
                aliases
                    .get(dependency)
                    .cloned()
                    .or_else(|| Some(dependency.clone()))
            })
            .collect();
        if item
            .dependencies
            .iter()
            .any(|dependency| !ids.contains(dependency))
        {
            return Err(StructureAnalysisError::Plan(
                "work item dependency is missing".into(),
            ));
        }
        item.dependencies.sort();
        item.dependencies.dedup();
        if item.demand.is_none() {
            item.demand = Some(build_demand(item)?);
        }
    }
    items.sort_by(|left, right| {
        left.priority
            .cmp(&right.priority)
            .then_with(|| {
                left.demand
                    .as_ref()
                    .and_then(|demand| demand.deadline_unix_ms)
                    .unwrap_or(u64::MAX)
                    .cmp(
                        &right
                            .demand
                            .as_ref()
                            .and_then(|demand| demand.deadline_unix_ms)
                            .unwrap_or(u64::MAX),
                    )
            })
            .then_with(|| left.id.cmp(&right.id))
    });
    let mut pending = items;
    let mut scheduled = Vec::with_capacity(pending.len());
    let mut completed = BTreeSet::new();
    while !pending.is_empty() {
        let index = pending
            .iter()
            .position(|item| item.dependencies.iter().all(|id| completed.contains(id)))
            .ok_or_else(|| StructureAnalysisError::Plan("work item dependency cycle".into()))?;
        let item = pending.remove(index);
        if !completed.insert(item.id.clone()) {
            return Err(StructureAnalysisError::Plan(
                "work item id has conflicting definitions".into(),
            ));
        }
        scheduled.push(item);
    }
    Ok(scheduled)
}

fn merge(
    existing: &mut StructureWorkItem,
    incoming: StructureWorkItem,
) -> Result<(), StructureAnalysisError> {
    let original = existing
        .demand
        .clone()
        .map_or_else(|| build_demand(existing), Ok)?;
    let incoming_demand = incoming
        .demand
        .clone()
        .map_or_else(|| build_demand(&incoming), Ok)?;
    if existing.source_revision != incoming.source_revision
        || existing.target_fingerprint != incoming.target_fingerprint
        || existing.adapter_id != incoming.adapter_id
        || existing.adapter_version != incoming.adapter_version
        || existing.intent != incoming.intent
    {
        return Err(StructureAnalysisError::Plan(
            "work-item merge identity is inconsistent".into(),
        ));
    }
    let target_size = existing.ranges.target_size();
    let mut ranges = existing.ranges.ranges().to_vec();
    ranges.extend_from_slice(incoming.ranges.ranges());
    existing.ranges = ByteRangeSet::new(target_size, ranges)
        .map_err(|error| StructureAnalysisError::Plan(error.to_string()))?;
    existing.priority = existing.priority.min(incoming.priority);
    existing.consumers.extend(incoming.consumers);
    existing.consumers.sort();
    existing.consumers.dedup();
    existing.dependencies.extend(incoming.dependencies);
    let mut demand = ReadDemand::new(
        existing.source_revision.clone(),
        existing.target_fingerprint.clone(),
        existing.intent,
        existing.ranges.clone(),
        existing.priority,
        original
            .deadline_unix_ms
            .into_iter()
            .chain(incoming_demand.deadline_unix_ms)
            .min(),
        original.reorderable && incoming_demand.reorderable,
        existing.consumers.clone(),
        original.verification,
    )
    .map_err(|error| StructureAnalysisError::Plan(error.to_string()))?;
    if let Some(attempt) = original.attempt {
        demand = demand
            .with_attempt(attempt)
            .map_err(|error| StructureAnalysisError::Plan(error.to_string()))?;
    }
    existing.demand = Some(demand);
    Ok(())
}

fn item_key(item: &StructureWorkItem) -> String {
    let ranges = item
        .ranges
        .ranges()
        .iter()
        .map(|range| format!("{}:{}", range.start(), range.length()))
        .collect::<Vec<_>>()
        .join(",");
    let identity = format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{:?}\u{1f}{}\u{1f}{}",
        item.source_revision.case_id,
        item.source_revision.source_id,
        item.source_revision.revision,
        item.target_fingerprint,
        item.adapter_id,
        item.adapter_version,
        item.intent,
        item.ranges.target_size(),
        ranges,
    );
    let policy = item
        .demand
        .as_ref()
        .map(|demand| (demand.verification, demand.attempt.clone()))
        .unwrap_or((domain::structure::ReadVerification::RangeCoverage, None));
    format!(
        "{identity}\u{1f}{}",
        serde_json::to_string(&policy).expect("serializable read policy")
    )
}

fn build_demand(item: &StructureWorkItem) -> Result<ReadDemand, StructureAnalysisError> {
    let deadline = item
        .demand
        .as_ref()
        .and_then(|demand| demand.deadline_unix_ms);
    let reorderable = item.demand.as_ref().is_none_or(|demand| demand.reorderable);
    let verification = item.demand.as_ref().map_or(
        domain::structure::ReadVerification::RangeCoverage,
        |demand| demand.verification,
    );
    let demand = ReadDemand::new(
        item.source_revision.clone(),
        item.target_fingerprint.clone(),
        item.intent,
        item.ranges.clone(),
        item.priority,
        deadline,
        reorderable,
        item.consumers.clone(),
        verification,
    )
    .map_err(|error| StructureAnalysisError::Plan(error.to_string()))?;
    let demand = match item
        .demand
        .as_ref()
        .and_then(|demand| demand.attempt.clone())
    {
        Some(attempt) => demand
            .with_attempt(attempt)
            .map_err(|error| StructureAnalysisError::Plan(error.to_string()))?,
        None => demand,
    };
    Ok(demand)
}
