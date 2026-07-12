use std::hash::{Hash, Hasher};

use super::model::MatchStrategy;

pub(super) fn secondary_normalize(value: &str, entity_type: &str) -> String {
    let value = value.trim().to_lowercase();
    match entity_type {
        "person" | "email" => value
            .split_once('@')
            .map_or(value.clone(), |(local, _)| local.to_string()),
        "account" => value
            .rsplit_once('\\')
            .map_or(value.clone(), |(_, account)| account.to_string()),
        _ => value,
    }
}

pub(super) fn match_id(entity_type: &str, seed: &str, strategy: &MatchStrategy) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    entity_type.hash(&mut hasher);
    seed.hash(&mut hasher);
    strategy.tag().hash(&mut hasher);
    format!("xcase:{}:{:016x}", strategy.tag(), hasher.finish())
}
