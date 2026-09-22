use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

const TOPOLOGY_SCOPE_ROOT: &str = "topology-scopes";

/// Returns the stable, filesystem-safe key for a typed topology scope.
///
/// Scope IDs are logical graph identifiers and may contain separators such as
/// `:`. They must never be used as path components directly, especially on
/// Windows where `:` has special meaning.
pub(crate) fn storage_key(scope_id: &str) -> String {
    hex::encode(Sha256::digest(scope_id.as_bytes()))
}

pub(crate) fn artifact_path(
    case_root: &Path,
    scope_id: &str,
    family: &str,
    file_name: &str,
) -> PathBuf {
    case_root
        .join(TOPOLOGY_SCOPE_ROOT)
        .join(family)
        .join(storage_key(scope_id))
        .join(file_name)
}

#[cfg(test)]
#[path = "../../tests/unit/cluster_service/scope_storage.rs"]
mod tests;
