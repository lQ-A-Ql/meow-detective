use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

/// Normalize an entity value for consistent deduplication and lookup.
pub fn normalize_entity_value(value: &str) -> String {
    value.trim().to_lowercase().nfkd().collect()
}

/// Hash a normalized entity value into the compact entity-index key.
pub fn hash_entity_value(normalized: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8])
}
