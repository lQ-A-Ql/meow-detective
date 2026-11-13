use super::super::{helpers::normalized_path_suffix_key, CorrelationRuleMatch};
use super::lookup::FileLookup;
use domain::FileEntry;
use std::collections::BTreeSet;

pub(crate) fn dedup_rule_matches(matches: &mut Vec<CorrelationRuleMatch>) {
    let mut seen = BTreeSet::new();
    matches.retain(|item| {
        seen.insert((
            item.artifact.id.clone(),
            item.file.id.0.clone(),
            item.kind.clone(),
        ))
    });
}

pub(crate) fn find_best_file_by_path<'a>(
    lookup: &FileLookup<'a>,
    candidate: &str,
    prefer_deleted: Option<bool>,
) -> Option<&'a FileEntry> {
    let normalized = normalize_path(candidate);
    if normalized.is_empty() {
        return None;
    }
    lookup
        .index()
        .exact_best(&normalized, prefer_deleted)
        .or_else(|| {
            lookup
                .index()
                .suffix_best(normalized_path_suffix_key(&normalized), prefer_deleted)
        })
        .map(|position| lookup.file(position))
}

pub(crate) fn find_best_file_by_name<'a>(
    lookup: &FileLookup<'a>,
    candidate: &str,
    prefer_deleted: Option<bool>,
) -> Option<&'a FileEntry> {
    let normalized = basename(candidate);
    if normalized.is_empty() {
        return None;
    }
    lookup
        .index()
        .basename_best(&normalized, prefer_deleted)
        .map(|position| lookup.file(position))
}

pub(crate) fn normalize_path(value: &str) -> String {
    persistence_source::fjall::instrumentation::record_path_normalization();
    let trimmed = value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches(|ch: char| matches!(ch, '(' | ')' | '[' | ']' | '<' | '>'));
    if trimmed.is_empty() {
        return String::new();
    }
    let mut normalized = trimmed.replace('\\', "/").to_ascii_lowercase();
    while normalized.contains("//") {
        normalized = normalized.replace("//", "/");
    }
    while normalized.ends_with('/') && normalized.len() > 3 {
        normalized.pop();
    }
    normalized
}

pub(crate) fn basename(value: &str) -> String {
    normalize_path(value)
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string()
}

pub(crate) fn looks_like_path(value: &str) -> bool {
    let candidate = value.trim();
    candidate.contains(":\\")
        || candidate.contains(":/")
        || candidate.starts_with("\\\\")
        || candidate.starts_with("//")
}
