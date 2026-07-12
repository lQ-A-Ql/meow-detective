use super::super::{
    parse_rfc3339_utc, path_suffix_key, CorrelationRuleGroup, CorrelationRuleMatch,
    RULE_TIMELINE_CONTEXT_LIMIT, RULE_TIMELINE_PROXIMITY_WINDOW_SECS,
};
use crate::correlation::rules::{
    rule_match_paths, rule_match_text_needles, rule_match_timestamps, timeline_path_candidates,
    timeline_text_candidates,
};
use chrono::Utc;
use std::collections::BTreeSet;
use transport::dto::TimelineEventDto;

pub(crate) fn derive_rule_timeline_signals(
    group: &CorrelationRuleGroup,
    all_timelines: &[TimelineEventDto],
) -> Vec<String> {
    let mut signals = own_timeline_signals(group);
    let artifact_times = group
        .matches
        .iter()
        .flat_map(rule_match_timestamps)
        .collect::<Vec<_>>();
    if artifact_times.is_empty() {
        return signals;
    }
    let (target_path_keys, text_needles) = collect_rule_match_needles(&group.matches);
    if target_path_keys.is_empty() && text_needles.is_empty() {
        return signals;
    }
    let mut related = find_proximate_timelines(
        all_timelines,
        &group.file.id.0,
        &artifact_times,
        &target_path_keys,
        &text_needles,
    );
    related.sort();
    related.truncate(RULE_TIMELINE_CONTEXT_LIMIT);
    signals.extend(
        related
            .into_iter()
            .map(|item| format!("邻近时间线命中 {item}")),
    );
    signals
}

fn own_timeline_signals(group: &CorrelationRuleGroup) -> Vec<String> {
    if group.timelines.is_empty() {
        Vec::new()
    } else {
        vec![format!(
            "关联文件自身已有 {} 条 timeline 事件",
            group.timelines.len()
        )]
    }
}

fn collect_rule_match_needles(
    matches: &[CorrelationRuleMatch],
) -> (BTreeSet<String>, BTreeSet<String>) {
    let target_path_keys = matches
        .iter()
        .flat_map(rule_match_paths)
        .map(|value| path_suffix_key(&value))
        .filter(|value| !value.is_empty())
        .collect();
    let text_needles = matches
        .iter()
        .flat_map(rule_match_text_needles)
        .map(|value| value.to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect();
    (target_path_keys, text_needles)
}

fn find_proximate_timelines(
    timelines: &[TimelineEventDto],
    excluded_source_id: &str,
    artifact_times: &[chrono::DateTime<Utc>],
    target_path_keys: &BTreeSet<String>,
    text_needles: &BTreeSet<String>,
) -> Vec<String> {
    timelines
        .iter()
        .filter(|timeline| timeline.source_object_id != excluded_source_id)
        .filter(|timeline| within_time_window(timeline, artifact_times))
        .filter(|timeline| timeline_matches(timeline, target_path_keys, text_needles))
        .map(|timeline| format!("{} @ {}", timeline.event_type, timeline.ts))
        .collect()
}

fn within_time_window(
    timeline: &TimelineEventDto,
    artifact_times: &[chrono::DateTime<Utc>],
) -> bool {
    let Some(timeline_ts) = parse_rfc3339_utc(&timeline.ts) else {
        return false;
    };
    artifact_times.iter().any(|artifact_ts| {
        (timeline_ts.timestamp() - artifact_ts.timestamp()).abs()
            <= RULE_TIMELINE_PROXIMITY_WINDOW_SECS
    })
}

fn timeline_matches(
    timeline: &TimelineEventDto,
    target_path_keys: &BTreeSet<String>,
    text_needles: &BTreeSet<String>,
) -> bool {
    let path_hit = timeline_path_candidates(timeline)
        .into_iter()
        .map(|candidate| path_suffix_key(&candidate))
        .any(|suffix| !suffix.is_empty() && target_path_keys.contains(&suffix));
    let text_hit = timeline_text_candidates(timeline)
        .into_iter()
        .map(|candidate| candidate.to_ascii_lowercase())
        .any(|candidate| {
            text_needles
                .iter()
                .any(|needle| !needle.is_empty() && candidate.contains(needle))
        });
    path_hit || text_hit
}
