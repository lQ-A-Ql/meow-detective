use std::path::Path;

use super::*;
use sha2::{Digest, Sha256};

fn document(pool_count: &str, digest: &str) -> String {
    format!(
        r#"{{"schemaVersion":1,"evidenceKind":"ceph_osdmap_poolmap","clusterId":"cluster-1","cephFsid":"11111111-1111-1111-1111-111111111111","cephRevision":"reef-18.2.1","epoch":17,"pools":{pool_count},"osds":[{{"osdId":1,"osdUuid":"22222222-2222-2222-2222-222222222222","up":true,"in":true,"weight":1000000,"address":"10.0.0.1:6800","cephFsid":"11111111-1111-1111-1111-111111111111"}}],"evidenceDigest":"{digest}"}}"#
    )
}

fn write_document(root: &Path, payload: &str) {
    let directory = root.join("clusters").join("cluster-1");
    std::fs::create_dir_all(&directory).expect("evidence directory");
    std::fs::write(directory.join("osdmap-evidence.json"), payload).expect("evidence document");
}

fn with_digest(mut payload: String) -> String {
    let marker = "\"evidenceDigest\":\"\"";
    payload = payload.replace("\"evidenceDigest\":\"placeholder\"", marker);
    let digest = {
        let mut hasher = Sha256::new();
        hasher.update(b"meow-detective-ceph-osdmap-poolmap-v1\0");
        hasher.update(payload.as_bytes());
        hex::encode(hasher.finalize())
    };
    payload.replace(marker, &format!("\"evidenceDigest\":\"{digest}\""))
}

#[test]
fn single_pool_evidence_binds_without_descriptor_pool() {
    let root = tempfile::TempDir::new().expect("root");
    let payload = with_digest(document(
        r#"[{"poolId":8,"poolType":"replicated","size":1,"minSize":1,"pgNum":8,"pgpNum":8}]"#,
        "placeholder",
    ));
    write_document(root.path(), &payload);
    let resolution = resolve_policy(root.path(), "cluster-1", None)
        .expect("valid evidence")
        .expect("OSDMap evidence");
    assert_eq!(resolution.policy.pool_evidence().unwrap().pool_id(), 8);
    assert_eq!(resolution.osd_count, 1);
}

#[test]
fn multiple_pools_require_descriptor_binding() {
    let root = tempfile::TempDir::new().expect("root");
    let payload = with_digest(document(
        r#"[{"poolId":7,"poolType":"replicated","size":1,"minSize":1,"pgNum":8,"pgpNum":8},{"poolId":8,"poolType":"replicated","size":1,"minSize":1,"pgNum":8,"pgpNum":8}]"#,
        "placeholder",
    ));
    write_document(root.path(), &payload);
    let error = resolve_policy(root.path(), "cluster-1", None).expect_err("ambiguous binding");
    assert_eq!(error, OsdMapEvidenceError::Invalid("pool binding"));
}

#[test]
fn digest_tampering_fails_closed() {
    let root = tempfile::TempDir::new().expect("root");
    let payload = document(
        r#"[{"poolId":8,"poolType":"replicated","size":1,"minSize":1,"pgNum":8,"pgpNum":8}]"#,
        &"0".repeat(64),
    );
    write_document(root.path(), &payload);
    let error = resolve_policy(root.path(), "cluster-1", Some(8)).expect_err("tampering");
    assert_eq!(error, OsdMapEvidenceError::DigestMismatch);
}

#[test]
fn unsupported_pool_type_is_rejected() {
    let root = tempfile::TempDir::new().expect("root");
    let payload = document(
        r#"[{"poolId":8,"poolType":"erasure","size":1,"minSize":1,"pgNum":8,"pgpNum":8}]"#,
        &"0".repeat(64),
    );
    write_document(root.path(), &payload);
    let error = resolve_policy(root.path(), "cluster-1", Some(8)).expect_err("EC unsupported");
    assert_eq!(error, OsdMapEvidenceError::UnsupportedPoolType);
}

#[test]
fn inventory_identity_must_match_the_epoch_bound_map() {
    let root = tempfile::TempDir::new().expect("root");
    let payload = with_digest(document(
        r#"[{"poolId":8,"poolType":"replicated","size":1,"minSize":1,"pgNum":8,"pgpNum":8}]"#,
        "placeholder",
    ));
    write_document(root.path(), &payload);
    let identity = ReplicaIdentity::from_inventory(
        Some(1),
        "22222222-2222-2222-2222-222222222222",
        Some("11111111-1111-1111-1111-111111111111".to_string()),
    );
    assert_eq!(
        validate_inventory_membership(root.path(), "cluster-1", &[identity])
            .expect("matching identity"),
        Some(1)
    );
}

#[test]
fn inventory_identity_mismatch_fails_closed() {
    let root = tempfile::TempDir::new().expect("root");
    let payload = with_digest(document(
        r#"[{"poolId":8,"poolType":"replicated","size":1,"minSize":1,"pgNum":8,"pgpNum":8}]"#,
        "placeholder",
    ));
    write_document(root.path(), &payload);
    let identity = ReplicaIdentity::from_inventory(
        Some(2),
        "22222222-2222-2222-2222-222222222222",
        Some("11111111-1111-1111-1111-111111111111".to_string()),
    );
    let error = validate_inventory_membership(root.path(), "cluster-1", &[identity])
        .expect_err("unknown OSD must fail closed");
    assert_eq!(error, OsdMapEvidenceError::ReplicaNotInMap);
}
