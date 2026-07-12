use super::super::{
    confidence_rank, CorrelationError, CorrelationRuleGroup, CorrelationSourceGroup,
};
use super::{derive_rule_timeline_signals, group_title, rule_group_confidence};
use crate::correlation::rules::build_artifact_rule_matches;
use domain::FileEntryId;
use persistence_sqlite::repositories::file_repo::FileRepo;
use rayon::prelude::*;
use rusqlite::Connection;
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::sync::Mutex;
use transport::dto::{ArtifactRowDto, TimelineEventDto};

pub(crate) fn build_source_groups(
    conn: &Connection,
    artifacts: Vec<ArtifactRowDto>,
    timelines: Vec<TimelineEventDto>,
) -> Result<Vec<CorrelationSourceGroup>, CorrelationError> {
    let groups = Mutex::new(BTreeMap::<String, CorrelationSourceGroup>::new());
    artifacts.par_iter().for_each(|artifact| {
        let Some(source_object_id) = artifact.source_object_id.as_ref() else {
            return;
        };
        let mut groups = groups.lock().unwrap_or_else(|error| error.into_inner());
        groups
            .entry(source_object_id.clone())
            .or_insert_with(|| source_group(source_object_id))
            .artifacts
            .push(artifact.clone());
    });
    timelines.par_iter().for_each(|timeline| {
        let mut groups = groups.lock().unwrap_or_else(|error| error.into_inner());
        groups
            .entry(timeline.source_object_id.clone())
            .or_insert_with(|| source_group(&timeline.source_object_id))
            .timelines
            .push(timeline.clone());
    });
    resolve_source_files(conn, groups.into_inner().unwrap())
}

fn source_group(source_object_id: &str) -> CorrelationSourceGroup {
    CorrelationSourceGroup {
        source_object_id: source_object_id.to_string(),
        ..CorrelationSourceGroup::default()
    }
}

fn resolve_source_files(
    conn: &Connection,
    groups: BTreeMap<String, CorrelationSourceGroup>,
) -> Result<Vec<CorrelationSourceGroup>, CorrelationError> {
    let repo = FileRepo::new(conn);
    let mut items = groups.into_values().collect::<Vec<_>>();
    for group in &mut items {
        group.file = repo
            .find_by_id(&FileEntryId(group.source_object_id.clone()))
            .map_err(|error| CorrelationError::Other(error.to_string()))?;
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
    let files = crate::analysis_service::collect_file_entries(conn)
        .map_err(|error| CorrelationError::Other(error.to_string()))?;
    let timeline_map = group_timelines(timelines);
    let groups = Mutex::new(BTreeMap::<String, CorrelationRuleGroup>::new());
    artifacts.par_iter().for_each(|artifact| {
        append_artifact_matches(
            &groups,
            &timeline_map,
            build_artifact_rule_matches(&files, artifact),
        );
    });
    finalize_rule_groups(groups.into_inner().unwrap(), timelines)
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

fn append_artifact_matches(
    groups: &Mutex<BTreeMap<String, CorrelationRuleGroup>>,
    timeline_map: &BTreeMap<String, Vec<TimelineEventDto>>,
    matches: Vec<super::super::CorrelationRuleMatch>,
) {
    for rule_match in matches {
        let file_id = rule_match.file.id.0.clone();
        let mut groups = groups.lock().unwrap_or_else(|error| error.into_inner());
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
