use std::path::Path;

use super::*;

fn write_document(root: &Path, cluster_id: &str, pools: &str) {
    let path = super::osdmap_evidence::evidence_path(root, cluster_id, "pool-evidence.json");
    let directory = path.parent().expect("evidence directory");
    std::fs::create_dir_all(&directory).expect("evidence directory");
    std::fs::write(
        path,
        format!(
            r#"{{"schemaVersion":1,"clusterId":"{cluster_id}","evidenceKind":"rbd_pool_replication","pools":{pools}}}"#
        ),
    )
    .expect("evidence document");
}

#[test]
fn typed_ceph_scope_without_map_evidence_falls_back_without_exposing_host_path() {
    let root = tempfile::TempDir::new().expect("root");
    let scope_id = "scope:ceph:18ea03d0-b1f3-4975-b552-12df7e0b53f1";
    let resolution =
        resolve_rbd_replica_policy(root.path(), scope_id, None, 3).expect("legacy fallback");

    assert_eq!(resolution.policy, RbdReplicaPolicy::strict_legacy());
    assert_eq!(resolution.diagnostics.len(), 1);
    assert!(resolution.diagnostics[0].contains("fallback"));
    assert!(!resolution.diagnostics[0].contains(root.path().to_string_lossy().as_ref()));
}

#[test]
fn single_pool_document_produces_trusted_policy() {
    let root = tempfile::TempDir::new().expect("root");
    write_document(
        root.path(),
        "cluster-1",
        r#"[{"poolId":8,"size":3,"minSize":2,"source":"osdmap:epoch-17","epoch":17,"evidenceDigest":null}]"#,
    );

    let resolution =
        resolve_rbd_replica_policy(root.path(), "cluster-1", None, 3).expect("trusted policy");
    assert!(resolution.diagnostics.is_empty());
    let evidence = resolution.policy.pool_evidence().expect("pool evidence");
    assert_eq!(evidence.pool_id(), 8);
    assert_eq!(evidence.min_size(), 2);
    assert_eq!(evidence.epoch(), Some(17));
}

#[test]
fn pool_binding_selects_matching_record_and_ambiguous_binding_fails_closed() {
    let root = tempfile::TempDir::new().expect("root");
    write_document(
        root.path(),
        "cluster-1",
        r#"[{"poolId":7,"size":3,"minSize":1,"source":"poolmap:7","epoch":10,"evidenceDigest":null},{"poolId":8,"size":3,"minSize":1,"source":"poolmap:8","epoch":10,"evidenceDigest":null}]"#,
    );

    let selected =
        resolve_rbd_replica_policy(root.path(), "cluster-1", Some(8), 3).expect("selected policy");
    assert_eq!(selected.policy.pool_evidence().unwrap().pool_id(), 8);

    let error = resolve_rbd_replica_policy(root.path(), "cluster-1", None, 3)
        .expect_err("ambiguous pool evidence must fail closed");
    assert!(matches!(error, PoolEvidenceError::PoolBindingRequired));

    let missing = resolve_rbd_replica_policy(root.path(), "cluster-1", Some(9), 3)
        .expect_err("missing pool evidence must fail closed");
    assert!(matches!(
        missing,
        PoolEvidenceError::PoolNotFound { pool_id: 9 }
    ));
}

#[test]
fn present_evidence_with_replica_count_mismatch_fails_closed() {
    let root = tempfile::TempDir::new().expect("root");
    write_document(
        root.path(),
        "cluster-1",
        r#"[{"poolId":8,"size":2,"minSize":1,"source":"osdmap:epoch-17","epoch":17,"evidenceDigest":null}]"#,
    );

    let error = resolve_rbd_replica_policy(root.path(), "cluster-1", None, 3)
        .expect_err("mismatch must fail closed");
    assert!(matches!(error, PoolEvidenceError::ReplicaCountMismatch));
}

#[test]
fn malformed_or_tampered_document_is_rejected() {
    let root = tempfile::TempDir::new().expect("root");
    write_document(
        root.path(),
        "cluster-1",
        r#"[{"poolId":8,"size":3,"minSize":1,"source":"osdmap","epoch":null,"evidenceDigest":null},{"poolId":8,"size":3,"minSize":1,"source":"osdmap","epoch":null,"evidenceDigest":null}]"#,
    );
    let error = resolve_rbd_replica_policy(root.path(), "cluster-1", None, 3)
        .expect_err("duplicate pools must fail");
    assert!(matches!(error, PoolEvidenceError::DuplicatePool));

    std::fs::write(
        super::osdmap_evidence::evidence_path(root.path(), "cluster-1", "pool-evidence.json"),
        "not-json",
    )
    .expect("tamper document");
    let error = resolve_rbd_replica_policy(root.path(), "cluster-1", None, 3)
        .expect_err("invalid JSON must fail");
    assert!(matches!(error, PoolEvidenceError::Json));
}
