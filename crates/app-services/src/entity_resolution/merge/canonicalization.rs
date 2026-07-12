use std::hash::{Hash, Hasher};

use unicode_normalization::UnicodeNormalization;

pub(super) fn canonicalize_entity(value: &str, entity_type: &str) -> String {
    let mut canonical: String = value.trim().to_lowercase().nfkd().collect();
    let prefixes: &[&str] = match entity_type {
        "person" | "email" => &["mailto:"],
        "account" => &["sid:"],
        _ => &[],
    };
    for prefix in prefixes {
        if let Some(stripped) = canonical.strip_prefix(prefix) {
            canonical = stripped.to_string();
            break;
        }
    }
    canonical
}

pub(super) fn entity_type_from_tags(tags_json: &str) -> String {
    let tags: Vec<String> = serde_json::from_str(tags_json).unwrap_or_default();
    tags.into_iter()
        .find(|tag| tag != "entity")
        .unwrap_or_else(|| "unknown".to_string())
}

pub(super) fn confidence(source_count: usize) -> f64 {
    match source_count {
        0 | 1 => 0.70,
        2 => 0.85,
        _ => 0.95,
    }
}

pub(super) fn resolved_entity_id(
    case_id: &str,
    canonical_value: &str,
    entity_type: &str,
) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    case_id.hash(&mut hasher);
    canonical_value.hash(&mut hasher);
    entity_type.hash(&mut hasher);
    format!("resolved:{}:{:016x}", case_id, hasher.finish())
}
