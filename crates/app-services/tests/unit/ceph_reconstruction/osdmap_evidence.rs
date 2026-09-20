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
fn single_pool_evidence_requires_placement_proof() {
    let root = tempfile::TempDir::new().expect("root");
    let payload = with_digest(document(
        r#"[{"poolId":8,"poolType":"replicated","size":1,"minSize":1,"pgNum":8,"pgpNum":8}]"#,
        "placeholder",
    ));
    write_document(root.path(), &payload);
    let error = resolve_policy(root.path(), "cluster-1", None).expect_err("placement proof");
    assert_eq!(error, OsdMapEvidenceError::PlacementNotProven);
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
fn historical_epoch_binding_uses_the_historical_epoch() {
    let root = tempfile::TempDir::new().expect("root");
    let mut parsed: EvidenceDocument = serde_json::from_str(&with_digest(document(
        r#"[{"poolId":8,"poolType":"replicated","size":1,"minSize":1,"pgNum":8,"pgpNum":8}]"#,
        "placeholder",
    )))
    .expect("fixture JSON");
    let mut historical = parsed.epoch_history[0].clone();
    historical.epoch = 16;
    historical.osdmap_payload_digest =
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd".to_string();
    historical.poolmap_payload_digest =
        "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".to_string();
    historical.map_binding_digest = binding_digest_for(
        &parsed,
        historical.epoch,
        &historical.osdmap_payload_digest,
        &historical.poolmap_payload_digest,
    );
    parsed.epoch_history.insert(0, historical);
    parsed.evidence_digest.clear();
    parsed.evidence_digest = canonical_digest(&mut parsed).expect("canonical digest");
    let payload = serde_json::to_string(&parsed).expect("fixture serialization");
    write_document(root.path(), &payload);
    assert_eq!(
        validate_inventory_membership(
            root.path(),
            "cluster-1",
            &[ReplicaIdentity::from_inventory(
                Some(1),
                "22222222-2222-2222-2222-222222222222",
                Some("11111111-1111-1111-1111-111111111111".to_string()),
            )],
        )
        .expect("historical map validation"),
        Some(1)
    );
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

#[test]
fn uuid_deduplication_is_canonical_and_case_insensitive() {
    let root = tempfile::TempDir::new().expect("root");
    let payload = document(
        r#"[{"poolId":8,"poolType":"replicated","size":1,"minSize":1,"pgNum":8,"pgpNum":8}]"#,
        "placeholder",
    )
    .replace(
        "\"osdId\":1,\"osdUuid\":\"22222222-2222-2222-2222-222222222222\"",
        "\"osdId\":1,\"osdUuid\":\"22222222-2222-2222-2222-222222222222\"",
    );
    let mut parsed: EvidenceDocument =
        serde_json::from_str(&with_digest(payload)).expect("fixture JSON");
    parsed.osds[0].osd_uuid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".to_string();
    parsed.osds.push(OsdRecord {
        osd_id: 2,
        osd_uuid: "AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE".to_string(),
        up: true,
        in_cluster: true,
        weight: 1_000_000,
        address: "10.0.0.2:6800".to_string(),
        ceph_fsid: parsed.ceph_fsid.clone(),
    });
    parsed.evidence_digest.clear();
    parsed.evidence_digest = canonical_digest(&mut parsed).expect("canonical digest");
    write_document(
        root.path(),
        &serde_json::to_string(&parsed).expect("fixture serialization"),
    );
    let error = validate_inventory_membership(
        root.path(),
        "cluster-1",
        &[ReplicaIdentity::from_inventory(
            Some(1),
            "22222222-2222-2222-2222-222222222222",
            Some("11111111-1111-1111-1111-111111111111".to_string()),
        )],
    )
    .expect_err("duplicate UUID");
    assert_eq!(error, OsdMapEvidenceError::Duplicate("OSD UUIDs"));
}

#[test]
fn oversized_evidence_is_rejected_before_json_parsing() {
    let root = tempfile::TempDir::new().expect("root");
    let directory = root.path().join("clusters").join("cluster-1");
    std::fs::create_dir_all(&directory).expect("evidence directory");
    std::fs::write(
        directory.join("osdmap-evidence.json"),
        vec![b'{'; 8 * 1024 * 1024 + 1],
    )
    .expect("evidence document");
    let error = validate_inventory_membership(root.path(), "cluster-1", &[])
        .expect_err("oversized evidence");
    assert_eq!(error, OsdMapEvidenceError::Invalid("evidence size"));
}
