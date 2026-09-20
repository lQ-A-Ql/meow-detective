use std::path::Path;

use super::*;

fn write_document(root: &Path, cluster_id: &str, pools: &str) {
    let directory = root.join("clusters").join(cluster_id);
    std::fs::create_dir_all(&directory).expect("evidence directory");
    std::fs::write(
        directory.join("pool-evidence.json"),
        format!(
            r#"{{"schemaVersion":1,"clusterId":"{cluster_id}","evidenceKind":"rbd_pool_replication","pools":{pools}}}"#
        ),
    )
    .expect("evidence document");
}

#[test]
fn missing_document_falls_back_without_exposing_host_path() {
    let root = tempfile::TempDir::new().expect("root");
    let resolution =
        resolve_rbd_replica_policy(root.path(), "cluster-1", None, 3).expect("legacy fallback");

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
fn pool_binding_selects_matching_record_and_missing_binding_falls_back() {
    let root = tempfile::TempDir::new().expect("root");
    write_document(
        root.path(),
        "cluster-1",
        r#"[{"poolId":7,"size":3,"minSize":1,"source":"poolmap:7","epoch":10,"evidenceDigest":null},{"poolId":8,"size":3,"minSize":1,"source":"poolmap:8","epoch":10,"evidenceDigest":null}]"#,
    );

    let selected =
        resolve_rbd_replica_policy(root.path(), "cluster-1", Some(8), 3).expect("selected policy");
    assert_eq!(selected.policy.pool_evidence().unwrap().pool_id(), 8);

    let fallback =
        resolve_rbd_replica_policy(root.path(), "cluster-1", None, 3).expect("ambiguous fallback");
    assert_eq!(fallback.policy, RbdReplicaPolicy::strict_legacy());
    assert!(fallback.diagnostics[0].contains("multiple"));

    let missing = resolve_rbd_replica_policy(root.path(), "cluster-1", Some(9), 3)
        .expect("missing pool fallback");
    assert_eq!(missing.policy, RbdReplicaPolicy::strict_legacy());
    assert!(missing.diagnostics[0].contains("absent"));
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
        root.path().join("clusters/cluster-1/pool-evidence.json"),
        "not-json",
    )
    .expect("tamper document");
    let error = resolve_rbd_replica_policy(root.path(), "cluster-1", None, 3)
        .expect_err("invalid JSON must fail");
    assert!(matches!(error, PoolEvidenceError::Json));
}
