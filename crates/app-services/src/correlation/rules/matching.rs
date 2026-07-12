use super::super::{deleted_preference_score, path_suffix_key, CorrelationRuleMatch};
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
    files: &'a [FileEntry],
    candidate: &str,
    prefer_deleted: Option<bool>,
) -> Option<&'a FileEntry> {
    let normalized = normalize_path(candidate);
    if normalized.is_empty() {
        return None;
    }
    if let Some(exact) = find_exact_path(files, &normalized, prefer_deleted) {
        return Some(exact);
    }
    let suffix = path_suffix_key(&normalized);
    if suffix.is_empty() {
        return None;
    }
    files
        .iter()
        .filter(|file| eligible_file(file, prefer_deleted))
        .filter(|file| path_suffix_key(&file.path).ends_with(&suffix))
        .min_by_key(|file| {
            (
                deleted_preference_score(file, prefer_deleted),
                file.path.len(),
            )
        })
}

fn find_exact_path<'a>(
    files: &'a [FileEntry],
    normalized: &str,
    prefer_deleted: Option<bool>,
) -> Option<&'a FileEntry> {
    files
        .iter()
        .filter(|file| eligible_file(file, prefer_deleted))
        .filter(|file| normalize_path(&file.path) == normalized)
        .min_by_key(|file| {
            (
                deleted_preference_score(file, prefer_deleted),
                file.path.len(),
            )
        })
}

pub(crate) fn find_best_file_by_name<'a>(
    files: &'a [FileEntry],
    candidate: &str,
    prefer_deleted: Option<bool>,
) -> Option<&'a FileEntry> {
    let normalized = basename(candidate);
    if normalized.is_empty() {
        return None;
    }
    files
        .iter()
        .filter(|file| eligible_file(file, prefer_deleted))
        .filter(|file| file.name.eq_ignore_ascii_case(&normalized))
        .min_by_key(|file| {
            (
                deleted_preference_score(file, prefer_deleted),
                file.path.len(),
            )
        })
}

fn eligible_file(file: &FileEntry, prefer_deleted: Option<bool>) -> bool {
    file.is_file() && deleted_preference_score(file, prefer_deleted) < 2
}

pub(crate) fn normalize_path(value: &str) -> String {
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
