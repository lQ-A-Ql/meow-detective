use super::super::{
    confidence_rank, CorrelationError, CorrelationRuleGroup, CorrelationSourceGroup,
};
use super::{derive_rule_timeline_signals, group_title, rule_group_confidence};
use crate::correlation::rules::{build_artifact_rule_matches, FileLookup};
use domain::FileEntryId;
use persistence_source::repositories::file_repo::FileRepo;
use rayon::prelude::*;
use rusqlite::Connection;
use std::cmp::Reverse;
use std::collections::BTreeMap;
use transport::dto::{ArtifactRowDto, TimelineEventDto};

pub(crate) fn build_source_groups(
    conn: &Connection,
    artifacts: Vec<ArtifactRowDto>,
    timelines: Vec<TimelineEventDto>,
) -> Result<Vec<CorrelationSourceGroup>, CorrelationError> {
    // Thread-local maps merged by `reduce`, no shared lock on the hot path.
    let artifact_groups = artifacts
        .par_iter()
        .filter_map(|artifact| {
            artifact
                .source_object_id
                .as_ref()
                .map(|source_object_id| (source_object_id, artifact))
        })
        .fold(BTreeMap::new, |mut groups, (source_object_id, artifact)| {
            groups
                .entry(source_object_id.clone())
                .or_insert_with(|| source_group(source_object_id))
                .artifacts
                .push(artifact.clone());
            groups
        })
        .reduce(BTreeMap::new, merge_source_groups);
    let timeline_groups = timelines
        .par_iter()
        .fold(BTreeMap::new, |mut groups, timeline| {
            groups
                .entry(timeline.source_object_id.clone())
                .or_insert_with(|| source_group(&timeline.source_object_id))
                .timelines
                .push(timeline.clone());
            groups
        })
        .reduce(BTreeMap::new, merge_source_groups);
    resolve_source_files(conn, merge_source_groups(artifact_groups, timeline_groups))
}

fn source_group(source_object_id: &str) -> CorrelationSourceGroup {
    CorrelationSourceGroup {
        source_object_id: source_object_id.to_string(),
        ..CorrelationSourceGroup::default()
    }
}

fn merge_source_groups(
    mut left: BTreeMap<String, CorrelationSourceGroup>,
    right: BTreeMap<String, CorrelationSourceGroup>,
) -> BTreeMap<String, CorrelationSourceGroup> {
    for (source_object_id, mut group) in right {
        let target = left
            .entry(source_object_id.clone())
            .or_insert_with(|| source_group(&source_object_id));
        target.artifacts.append(&mut group.artifacts);
        target.timelines.append(&mut group.timelines);
    }
    left
}

fn resolve_source_files(
    conn: &Connection,
    groups: BTreeMap<String, CorrelationSourceGroup>,
) -> Result<Vec<CorrelationSourceGroup>, CorrelationError> {
    let ids = groups
        .keys()
        .map(|id| FileEntryId(id.clone()))
        .collect::<Vec<_>>();
    let files = FileRepo::new(conn)
        .find_by_ids(&ids)
        .map_err(|error| CorrelationError::Other(error.to_string()))?;
    let mut items = groups.into_values().collect::<Vec<_>>();
    for group in &mut items {
        group.file = files.get(&group.source_object_id).cloned();
        group
            .artifacts
            .sort_by(|left, right| left.id.cmp(&right.id));
        group
            .timelines
            .sort_by(|left, right| left.id.cmp(&right.id));
    }
    items.sort_by_key(|group| {
        (
            Reverse(group.artifacts.len() + group.timelines.len()),
            Reverse(group.artifacts.len()),
            group_title(group),
        )
    });
    Ok(items)
}

pub(crate) fn build_rule_groups(
    conn: &Connection,
    artifacts: &[ArtifactRowDto],
    timelines: &[TimelineEventDto],
) -> Result<Vec<CorrelationRuleGroup>, CorrelationError> {
    if artifacts.is_empty() {
        return Ok(Vec::new());
    }
    let files = crate::analysis_service::collect_file_entries(conn)
        .map_err(|error| CorrelationError::Other(error.to_string()))?;
    // One normalized path index for the whole pass: matching each artifact is
    // then a hash lookup instead of a scan over the file catalog.
    let lookup = FileLookup::new(&files);
    let timeline_map = group_timelines(timelines);
    let matches = artifacts
        .par_iter()
        .flat_map_iter(|artifact| build_artifact_rule_matches(&lookup, artifact))
        .collect::<Vec<_>>();
    let groups = group_rule_matches(matches, &timeline_map);
    finalize_rule_groups(groups, timelines)
}

fn group_timelines(timelines: &[TimelineEventDto]) -> BTreeMap<String, Vec<TimelineEventDto>> {
    timelines.iter().fold(BTreeMap::new(), |mut groups, item| {
        groups
            .entry(item.source_object_id.clone())
            .or_default()
            .push(item.clone());
        groups
    })
}

fn group_rule_matches(
    matches: Vec<super::super::CorrelationRuleMatch>,
    timeline_map: &BTreeMap<String, Vec<TimelineEventDto>>,
) -> BTreeMap<String, CorrelationRuleGroup> {
    let mut groups = BTreeMap::<String, CorrelationRuleGroup>::new();
    for rule_match in matches {
        let file_id = rule_match.file.id.0.clone();
        let group = groups
            .entry(file_id.clone())
            .or_insert_with(|| CorrelationRuleGroup {
                file: rule_match.file.clone(),
                matches: Vec::new(),
                timelines: timeline_map.get(&file_id).cloned().unwrap_or_default(),
                timeline_signals: Vec::new(),
            });
        if !group.matches.iter().any(|existing| {
            existing.artifact.id == rule_match.artifact.id
                && existing.kind == rule_match.kind
                && existing.file.id == rule_match.file.id
        }) {
            group.matches.push(rule_match);
        }
    }
    groups
}

fn finalize_rule_groups(
    groups: BTreeMap<String, CorrelationRuleGroup>,
    timelines: &[TimelineEventDto],
) -> Result<Vec<CorrelationRuleGroup>, CorrelationError> {
    let mut items = groups.into_values().collect::<Vec<_>>();
    for group in &mut items {
        group.timeline_signals = derive_rule_timeline_signals(group, timelines);
        group.matches.sort_by_key(|rule| {
            (
                Reverse(confidence_rank(&rule.confidence)),
                rule.artifact.artifact_type.clone(),
                rule.artifact.id.clone(),
            )
        });
    }
    items.sort_by_key(|group| {
        (
            Reverse(confidence_rank(&rule_group_confidence(
                &group.matches,
                group.timelines.len() as u32,
                !group.timeline_signals.is_empty(),
            ))),
            Reverse(group.matches.len()),
            group.file.path.clone(),
        )
    });
    Ok(items)
}
