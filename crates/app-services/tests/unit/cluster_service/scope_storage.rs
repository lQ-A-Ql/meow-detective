use std::path::Path;

use super::super::scope_storage::{artifact_path, storage_key};

#[test]
fn scope_storage_key_is_stable_and_never_contains_logical_id_text() {
    let scope_id = "scope:ceph:18ea03d0-b1f3-4975-b552-12df7e0b53f1";
    let key = storage_key(scope_id);
    assert_eq!(key.len(), 64);
    assert!(key.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert!(!key.contains("scope"));
}

#[test]
fn topology_artifacts_use_a_filesystem_safe_scope_component() {
    let root = Path::new("D:/cases/case-1");
    let key = storage_key("scope:kubernetes:case-1");
    let path = artifact_path(
        root,
        "scope:kubernetes:case-1",
        "kubernetes",
        "inventory.json",
    );
    assert_eq!(
        path.parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str()),
        Some(key.as_str())
    );
    assert!(!path.to_string_lossy().contains("scope:kubernetes:"));
}
