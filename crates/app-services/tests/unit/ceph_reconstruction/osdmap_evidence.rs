use std::path::Path;

use super::*;

fn document(pool_count: &str, digest: &str) -> String {
    format!(
        r#"{{"schemaVersion":2,"evidenceKind":"ceph_osdmap_poolmap","clusterId":"cluster-1","cephFsid":"11111111-1111-1111-1111-111111111111","cephRevision":"reef-18.2.1","epoch":17,"pools":{pool_count},"osds":[{{"osdId":1,"osdUuid":"22222222-2222-2222-2222-222222222222","up":true,"in":true,"weight":1000000,"address":"10.0.0.1:6800","cephFsid":"11111111-1111-1111-1111-111111111111"}}],"evidenceDigest":"{digest}","source":{{"sourceKind":"offline_snapshot","sourceId":"snapshot-17"}},"osdmapPayloadDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","poolmapPayloadDigest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","mapBindingDigest":"placeholder-binding","epochHistory":[{{"epoch":17,"osdmapPayloadDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","poolmapPayloadDigest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","mapBindingDigest":"placeholder-binding"}}]}}"#
    )
}

fn write_document(root: &Path, payload: &str) {
    let directory = root.join("clusters").join("cluster-1");
    std::fs::create_dir_all(&directory).expect("evidence directory");
    std::fs::write(directory.join("osdmap-evidence.json"), payload).expect("evidence document");
}

fn with_digest(payload: String) -> String {
    let mut parsed: EvidenceDocument = serde_json::from_str(&payload).expect("fixture JSON");
    let binding = binding_digest_for(
        &parsed,
        parsed.epoch,
        &parsed.osdmap_payload_digest,
        &parsed.poolmap_payload_digest,
    );
    parsed.map_binding_digest = binding.clone();
    parsed.epoch_history[0].map_binding_digest = binding;
    parsed.evidence_digest.clear();
    parsed.evidence_digest = canonical_digest(&mut parsed).expect("canonical digest");
    serde_json::to_string(&parsed).expect("fixture serialization")
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
    let mut parsed: EvidenceDocument = serde_json::from_str(&with_digest(document(
        r#"[{"poolId":8,"poolType":"replicated","size":1,"minSize":1,"pgNum":8,"pgpNum":8}]"#,
        "placeholder",
    )))
    .expect("fixture JSON");
    parsed.evidence_digest = "0".repeat(64);
    let payload = serde_json::to_string(&parsed).expect("fixture serialization");
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

#[test]
fn legacy_schema_is_rejected_without_compatibility_fallback() {
    let root = tempfile::TempDir::new().expect("root");
    let payload = document(
        r#"[{"poolId":8,"poolType":"replicated","size":1,"minSize":1,"pgNum":8,"pgpNum":8}]"#,
        "placeholder",
    )
    .replace("\"schemaVersion\":2", "\"schemaVersion\":1");
    write_document(root.path(), &payload);
    let error = resolve_policy(root.path(), "cluster-1", Some(8)).expect_err("legacy schema");
    assert_eq!(error, OsdMapEvidenceError::Invalid("schema version"));
}

#[test]
fn epoch_history_must_be_strictly_increasing() {
    let root = tempfile::TempDir::new().expect("root");
    let mut parsed: EvidenceDocument = serde_json::from_str(&with_digest(document(
        r#"[{"poolId":8,"poolType":"replicated","size":1,"minSize":1,"pgNum":8,"pgpNum":8}]"#,
        "placeholder",
    )))
    .expect("fixture JSON");
    parsed.epoch_history.push(parsed.epoch_history[0].clone());
    parsed.evidence_digest.clear();
    parsed.evidence_digest = canonical_digest(&mut parsed).expect("canonical digest");
    let payload = serde_json::to_string(&parsed).expect("fixture serialization");
    write_document(root.path(), &payload);
    let error = resolve_policy(root.path(), "cluster-1", Some(8)).expect_err("epoch binding");
    assert_eq!(error, OsdMapEvidenceError::Invalid("epoch monotonicity"));
}

#[test]
fn map_binding_digest_binds_fsid_revision_and_payloads() {
    let root = tempfile::TempDir::new().expect("root");
    let mut parsed: EvidenceDocument = serde_json::from_str(&with_digest(document(
        r#"[{"poolId":8,"poolType":"replicated","size":1,"minSize":1,"pgNum":8,"pgpNum":8}]"#,
        "placeholder",
    )))
    .expect("fixture JSON");
    parsed.osdmap_payload_digest =
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string();
    parsed.evidence_digest.clear();
    parsed.evidence_digest = canonical_digest(&mut parsed).expect("canonical digest");
    let payload = serde_json::to_string(&parsed).expect("fixture serialization");
    write_document(root.path(), &payload);
    let error = resolve_policy(root.path(), "cluster-1", Some(8)).expect_err("binding mismatch");
    assert_eq!(error, OsdMapEvidenceError::Invalid("map binding digest"));
}
