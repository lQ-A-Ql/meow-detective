use std::collections::BTreeSet;

use domain::structure::{
    SnapshotAnalysisReceipt, SnapshotAnalysisReceiptError, SourceRevisionRef, StructureAttemptRef,
};
use sha2::{Digest, Sha256};

#[test]
fn snapshot_receipt_round_trip_preserves_exact_unsigned_revisions() {
    let original = receipt(u64::MAX);
    original.validate().unwrap();
    let encoded = serde_json::to_string(&original).unwrap();
    assert!(encoded.contains("18446744073709551615"));
    assert_eq!(
        serde_json::from_str::<SnapshotAnalysisReceipt>(&encoded).unwrap(),
        original
    );
}

#[test]
fn snapshot_receipt_contains_pure_metadata_without_byte_verification_claims() {
    let value = serde_json::to_value(receipt(1)).unwrap();
    let keys = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        keys,
        BTreeSet::from([
            "sourceRevision",
            "dependencyRevisions",
            "targetId",
            "inputCapabilityIds",
            "generationIdentity",
            "scopeFingerprint",
            "outputSha256",
            "attempt",
            "verifierId",
            "policyVersion",
            "id",
        ])
    );
}

#[test]
fn constructor_normalizes_dependency_and_input_capability_sets() {
    let original = receipt(7);
    let mut reordered = original.clone();
    reordered.dependency_revisions.reverse();
    reordered
        .dependency_revisions
        .push(reordered.source_revision.clone());
    reordered.input_capability_ids = ["capability-b@2", "capability-a@1", "capability-b@2"]
        .map(String::from)
        .into_iter()
        .collect();
    let normalized = rebuild(reordered).unwrap();
    assert_eq!(normalized, original);
}

#[test]
fn adjacent_revisions_above_float_precision_have_distinct_receipt_identities() {
    let first = receipt(9_007_199_254_740_992);
    let second = receipt(9_007_199_254_740_993);
    assert_ne!(first.id, second.id);
    let encoded = serde_json::to_string(&second).unwrap();
    assert!(encoded.contains("9007199254740993"));
    assert_eq!(
        serde_json::from_str::<SnapshotAnalysisReceipt>(&encoded).unwrap(),
        second
    );
}

#[test]
fn output_digest_binds_the_exact_serialized_bytes_without_canonicalizing_them() {
    let output = serde_json::json!({"count": 2});
    let encoded = serde_json::to_vec(&output).unwrap();
    assert_eq!(encoded, br#"{"count":2}"#);
    let compact_digest = hex::encode(Sha256::digest(br#"{"count":2}"#));
    let spaced_digest = hex::encode(Sha256::digest(br#"{"count": 2}"#));
    assert_ne!(compact_digest, spaced_digest);
    let mut first = receipt(1);
    first.output_sha256 = hex::encode(Sha256::digest(&encoded));
    let first = rebuild(first).unwrap();
    assert_eq!(first.output_sha256, compact_digest);
    let mut second = first.clone();
    second.output_sha256 = spaced_digest;
    let second = rebuild(second).unwrap();
    assert_ne!(first.id, second.id);
}

#[test]
fn receipt_identity_binds_target_capabilities_generation_scope_and_output() {
    let original = receipt(1);
    let mut alternatives = vec![original.clone(); 7];
    alternatives[0].target_id = "other-target".into();
    alternatives[1].input_capability_ids = BTreeSet::from(["capability-a@2".into()]);
    alternatives[2].generation_identity = "generation-attempt-b".into();
    alternatives[3].scope_fingerprint = "c".repeat(64);
    alternatives[4].output_sha256 = "d".repeat(64);
    alternatives[5].verifier_id = "another-snapshot-analyzer".into();
    alternatives[6].policy_version = "snapshot-policy-2".into();
    for changed in alternatives {
        assert_ne!(original.id, rebuild(changed).unwrap().id);
    }
}

#[test]
fn receipt_identity_binds_owner_attempt_and_dependency_versions() {
    let original = receipt(1);
    let mut alternatives = vec![original.clone(); 3];
    alternatives[0].attempt.attempt_id = "attempt-b".into();
    alternatives[1].attempt.owner = "owner-b".into();
    alternatives[2].dependency_revisions[0].revision = 2;
    for changed in alternatives {
        assert_ne!(original.id, rebuild(changed).unwrap().id);
    }
}

#[test]
fn receipt_rejects_empty_required_fields_and_invalid_digests() {
    for field in [
        "targetId",
        "generationIdentity",
        "scopeFingerprint",
        "outputSha256",
        "verifierId",
        "policyVersion",
        "id",
    ] {
        let mut value = serde_json::to_value(receipt(1)).unwrap();
        value[field] = serde_json::json!("");
        assert!(serde_json::from_value::<SnapshotAnalysisReceipt>(value).is_err());
    }
    for field in ["scopeFingerprint", "outputSha256"] {
        for invalid in ["A".repeat(64), "g".repeat(64), "a".repeat(63)] {
            let mut value = serde_json::to_value(receipt(1)).unwrap();
            value[field] = serde_json::json!(invalid);
            assert!(serde_json::from_value::<SnapshotAnalysisReceipt>(value).is_err());
        }
    }
    for field in ["attemptId", "owner"] {
        let mut value = serde_json::to_value(receipt(1)).unwrap();
        value["attempt"][field] = serde_json::json!(" ");
        assert!(serde_json::from_value::<SnapshotAnalysisReceipt>(value).is_err());
    }
}

#[test]
fn receipt_rejects_missing_or_blank_capability_references() {
    for capabilities in [serde_json::json!([]), serde_json::json!([" "])] {
        let mut value = serde_json::to_value(receipt(1)).unwrap();
        value["inputCapabilityIds"] = capabilities;
        assert!(serde_json::from_value::<SnapshotAnalysisReceipt>(value).is_err());
    }
    let mut invalid = receipt(1);
    invalid.input_capability_ids.clear();
    assert!(matches!(
        invalid.validate(),
        Err(SnapshotAnalysisReceiptError::MissingCapabilities)
    ));
}

#[test]
fn receipt_requires_one_current_revision_per_same_case_dependency() {
    let original = receipt(1);
    let mut invalid = original.clone();
    invalid.dependency_revisions[0].case_id = "foreign-case".into();
    assert!(matches!(
        invalid.validate(),
        Err(SnapshotAnalysisReceiptError::CrossCaseDependency)
    ));
    let mut invalid = original.clone();
    invalid
        .dependency_revisions
        .retain(|reference| reference != &original.source_revision);
    assert!(matches!(
        invalid.validate(),
        Err(SnapshotAnalysisReceiptError::MissingSourceDependency)
    ));
    let mut invalid = original;
    let mut conflicting = invalid.dependency_revisions[0].clone();
    conflicting.revision += 1;
    invalid.dependency_revisions.push(conflicting);
    assert!(matches!(
        rebuild(invalid),
        Err(SnapshotAnalysisReceiptError::ConflictingDependencyRevision)
    ));
}

#[test]
fn serialized_dependency_order_and_duplicates_cannot_silently_change_the_receipt() {
    let original = receipt(1);
    for dependencies in [
        serde_json::json!([
            original.dependency_revisions[1],
            original.dependency_revisions[0]
        ]),
        serde_json::json!([
            original.dependency_revisions[0],
            original.dependency_revisions[0],
            original.dependency_revisions[1]
        ]),
    ] {
        let mut value = serde_json::to_value(&original).unwrap();
        value["dependencyRevisions"] = dependencies;
        assert!(serde_json::from_value::<SnapshotAnalysisReceipt>(value).is_err());
    }
}

#[test]
fn changed_receipt_fields_require_a_new_canonical_identity() {
    let original = receipt(1);
    for field in [
        "targetId",
        "generationIdentity",
        "verifierId",
        "policyVersion",
    ] {
        let mut value = serde_json::to_value(&original).unwrap();
        value[field] = serde_json::json!("changed-valid-identity");
        assert!(serde_json::from_value::<SnapshotAnalysisReceipt>(value).is_err());
    }
    let mut changed = original;
    changed.output_sha256 = "e".repeat(64);
    assert!(matches!(
        changed.validate(),
        Err(SnapshotAnalysisReceiptError::IdentityMismatch)
    ));
}

#[test]
fn receipt_rejects_unknown_fields_at_root_revision_and_attempt_boundaries() {
    for path in ["", "/sourceRevision", "/dependencyRevisions/0", "/attempt"] {
        let mut value = serde_json::to_value(receipt(1)).unwrap();
        value.pointer_mut(path).unwrap()["unknownField"] = serde_json::json!(true);
        assert!(serde_json::from_value::<SnapshotAnalysisReceipt>(value).is_err());
    }
}

#[test]
fn receipt_rejects_missing_fields_unregistered_revisions_and_float_revisions() {
    let original = receipt(1);
    let fields = serde_json::to_value(&original)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    for field in fields {
        let mut value = serde_json::to_value(&original).unwrap();
        value.as_object_mut().unwrap().remove(&field);
        assert!(serde_json::from_value::<SnapshotAnalysisReceipt>(value).is_err());
    }
    for revision in [
        serde_json::json!(0),
        serde_json::json!(1.0),
        serde_json::json!(-1),
    ] {
        let mut value = serde_json::to_value(&original).unwrap();
        value["sourceRevision"]["revision"] = revision;
        assert!(serde_json::from_value::<SnapshotAnalysisReceipt>(value).is_err());
    }
}

fn receipt(revision: u64) -> SnapshotAnalysisReceipt {
    let source = SourceRevisionRef {
        case_id: "case-a".into(),
        source_id: "source-a".into(),
        revision,
    };
    SnapshotAnalysisReceipt::new(
        source.clone(),
        vec![
            source,
            SourceRevisionRef {
                case_id: "case-a".into(),
                source_id: "parent-a".into(),
                revision: 1,
            },
        ],
        "snapshot-target-a".into(),
        BTreeSet::from(["capability-a@1".into(), "capability-b@2".into()]),
        "fixed-generation-identity-a".into(),
        "a".repeat(64),
        hex::encode(Sha256::digest(b"null")),
        StructureAttemptRef {
            attempt_id: "attempt-a".into(),
            owner: "owner-a".into(),
        },
        "snapshot-analyzer-a".into(),
        "snapshot-policy-1".into(),
    )
    .unwrap()
}

fn rebuild(
    receipt: SnapshotAnalysisReceipt,
) -> Result<SnapshotAnalysisReceipt, SnapshotAnalysisReceiptError> {
    SnapshotAnalysisReceipt::new(
        receipt.source_revision,
        receipt.dependency_revisions,
        receipt.target_id,
        receipt.input_capability_ids,
        receipt.generation_identity,
        receipt.scope_fingerprint,
        receipt.output_sha256,
        receipt.attempt,
        receipt.verifier_id,
        receipt.policy_version,
    )
}
