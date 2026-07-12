use super::super::{first_string_attr, parse_rfc3339_utc, string_array_attr, CorrelationRuleMatch};
use super::looks_like_path;
use transport::dto::TimelineEventDto;

pub(crate) fn rule_match_timestamps(
    rule: &CorrelationRuleMatch,
) -> Vec<chrono::DateTime<chrono::Utc>> {
    let keys = match rule.artifact.artifact_type.as_str() {
        "BrowserHistory" => &["visitTime"][..],
        "BrowserDownload" => &["startTime"][..],
        "EmailMessage" => &["sentAt"][..],
        _ => return Vec::new(),
    };
    first_string_attr(&rule.artifact.attrs, keys)
        .and_then(|value| parse_rfc3339_utc(&value))
        .into_iter()
        .collect()
}

pub(crate) fn rule_match_paths(rule: &CorrelationRuleMatch) -> Vec<String> {
    let keys = match rule.artifact.artifact_type.as_str() {
        "BrowserDownload" => &["targetPath"][..],
        "JumpList" | "LNK" => &["target_path", "targetPath"][..],
        "RecycleBin" => &["original_path", "originalPath"][..],
        _ => return Vec::new(),
    };
    first_string_attr(&rule.artifact.attrs, keys)
        .into_iter()
        .collect()
}

pub(crate) fn rule_match_text_needles(rule: &CorrelationRuleMatch) -> Vec<String> {
    match rule.artifact.artifact_type.as_str() {
        "BrowserHistory" => string_attrs(rule, &["url", "title"]),
        "EmailMessage" => {
            let mut values = string_attrs(rule, &["subject"]);
            values.extend(string_array_attr(&rule.artifact.attrs, "attachments"));
            values
        }
        _ => Vec::new(),
    }
}

fn string_attrs(rule: &CorrelationRuleMatch, keys: &[&str]) -> Vec<String> {
    keys.iter()
        .filter_map(|key| first_string_attr(&rule.artifact.attrs, &[*key]))
        .collect()
}

pub(crate) fn timeline_path_candidates(timeline: &TimelineEventDto) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(value) = first_string_attr(&timeline.attrs, &["path", "targetPath", "sourcePath"]) {
        candidates.push(value);
    }
    if let Some(value) = timeline.source_attribution.clone() {
        if looks_like_path(&value) {
            candidates.push(value);
        }
    }
    candidates
}

pub(crate) fn timeline_text_candidates(timeline: &TimelineEventDto) -> Vec<String> {
    let mut candidates = vec![timeline.title.clone(), timeline.description.clone()];
    if let Some(value) = first_string_attr(&timeline.attrs, &["url", "title"]) {
        candidates.push(value);
    }
    candidates
}
