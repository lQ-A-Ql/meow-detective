use super::{basename, looks_like_path};
use std::collections::BTreeSet;

pub(crate) fn extract_path_candidates(value: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let trimmed = value.trim();
    if looks_like_path(trimmed) {
        candidates.push(trimmed.to_string());
    }
    for segment in extract_quoted_segments(trimmed) {
        if looks_like_path(&segment) {
            candidates.push(segment);
        }
    }
    for token in split_candidates(trimmed) {
        if looks_like_path(token) {
            candidates.push(token.to_string());
        }
    }
    normalize_candidates(candidates)
}

pub(crate) fn extract_file_name_candidates(value: &str) -> Vec<String> {
    let mut names = Vec::new();
    let trimmed = value.trim();
    let direct_name = basename(trimmed);
    if direct_name.contains('.') && !looks_like_path(trimmed) {
        names.push(direct_name);
    }
    for token in split_candidates(trimmed) {
        let name = basename(token);
        if name.contains('.') && !name.is_empty() {
            names.push(name);
        }
    }
    normalize_candidates(names)
}

fn split_candidates(value: &str) -> impl Iterator<Item = &str> {
    value.split(|ch: char| {
        ch.is_whitespace() || matches!(ch, ',' | ';' | '|' | '(' | ')' | '[' | ']')
    })
}

fn normalize_candidates(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|item| {
            item.trim_matches('"')
                .trim_matches('\'')
                .trim_end_matches('.')
                .trim_end_matches(',')
                .to_string()
        })
        .filter(|item| !item.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn extract_quoted_segments(value: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for ch in value.chars() {
        match quote {
            Some(active) if ch == active => {
                if !current.trim().is_empty() {
                    items.push(current.trim().to_string());
                }
                current.clear();
                quote = None;
            }
            Some(_) => current.push(ch),
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None => {}
        }
    }
    items
}
