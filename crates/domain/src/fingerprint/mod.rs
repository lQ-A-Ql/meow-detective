mod encoding;
mod metadata;
mod record;
mod types;

pub const FMD_SCHEMA_VERSION: &str = "fmd-v1";

pub use metadata::ForensicMetadata;
pub use record::{normalize_content_sha256, ForensicFingerprint};
pub use types::ForensicObjectType;

#[cfg(test)]
#[path = "../../tests/unit/fingerprint.rs"]
mod tests;
