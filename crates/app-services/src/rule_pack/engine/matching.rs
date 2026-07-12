use std::collections::BTreeMap;

use serde_json::Value;

use super::super::parser::{FieldPredicate, Operator};
use super::loading::{FileEntryRow, NodeRow};

pub(super) fn conditions_match(
    attrs: &BTreeMap<String, Value>,
    target: &NodeRow,
    files: &[FileEntryRow],
    conditions: &[FieldPredicate],
) -> bool {
    !conditions.is_empty()
        && conditions.iter().all(|condition| {
            let Some(source) = field_value(attrs, &condition.field) else {
                return false;
            };
            predicate_matches(&source, target, files, condition)
        })
}

fn predicate_matches(
    source: &str,
    target: &NodeRow,
    files: &[FileEntryRow],
    condition: &FieldPredicate,
) -> bool {
    let Some(target_value) = node_field_value(target, files, &condition.target_field) else {
        return false;
    };
    match condition.operator {
        Operator::Equals => source.eq_ignore_ascii_case(&target_value),
        Operator::Contains => target_value
            .to_ascii_lowercase()
            .contains(&source.to_ascii_lowercase()),
        Operator::PathEquals => paths_match(source, &target_value),
        Operator::FilenameEquals => filenames_match(source, &target_value),
        Operator::Regex => regex::Regex::new(source)
            .map(|regex| regex.is_match(&target_value))
            .unwrap_or(false),
        Operator::TemporalProximity => false,
    }
}

fn field_value(attrs: &BTreeMap<String, Value>, field: &str) -> Option<String> {
    attrs.get(field).and_then(|value| match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Array(values) => {
            let parts: Vec<&str> = values.iter().filter_map(Value::as_str).collect();
            (!parts.is_empty()).then(|| parts.join(" "))
        }
        _ => None,
    })
}

fn node_field_value(node: &NodeRow, files: &[FileEntryRow], target_field: &str) -> Option<String> {
    let file = files.iter().find(|file| file.id == node.id);
    match target_field {
        "label" | "name" => file
            .map(|file| file.name.clone())
            .or_else(|| Some(node.label.clone())),
        "path" => file.map(|file| file.path.clone()),
        "summary" => Some(node.summary.clone()),
        _ => None,
    }
}

fn paths_match(source: &str, target: &str) -> bool {
    let source = normalize_path(source);
    let target = normalize_path(target);
    !source.is_empty()
        && !target.is_empty()
        && (source == target || path_suffix_key(&source) == path_suffix_key(&target))
}

fn filenames_match(source: &str, target: &str) -> bool {
    let source = normalize_path(source);
    let target = normalize_path(target);
    let source_name = source.rsplit('/').next().unwrap_or(&source);
    let target_name = target.rsplit('/').next().unwrap_or(&target);
    !source_name.is_empty()
        && !target_name.is_empty()
        && source_name.eq_ignore_ascii_case(target_name)
}

fn normalize_path(value: &str) -> String {
    let trimmed = value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches(|character: char| matches!(character, '(' | ')' | '[' | ']' | '<' | '>'));
    let mut normalized = trimmed.replace('\\', "/").to_ascii_lowercase();
    while normalized.contains("//") {
        normalized = normalized.replace("//", "/");
    }
    while normalized.ends_with('/') && normalized.len() > 3 {
        normalized.pop();
    }
    normalized
}

fn path_suffix_key(value: &str) -> &str {
    let bytes = value.as_bytes();
    if value.len() >= 3 && bytes[1] == b':' && bytes[2] == b'/' {
        &value[3..]
    } else {
        value.trim_start_matches('/')
    }
}
