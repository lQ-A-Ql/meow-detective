use domain::structure::{
    ByteCoverage, ByteRange, CapabilityError, CapabilityIntegrity, CapabilityKind, CapabilityRef,
    CoverageContext, CoverageError, NamespaceCoverage, NamespacePage, NamespacePageEnd,
    TargetCoverage, VersionBoundary,
};
use serde_json::json;
use sha2::{Digest, Sha256};

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn metadata_coverage() -> TargetCoverage {
    let header = ByteRange::new(4096, 512).unwrap();
    TargetCoverage::Bytes(ByteCoverage {
        target_size: 1024 * 1024,
        requested: vec![header],
        verified: vec![header],
        native_holes: Vec::new(),
        unknown: Vec::new(),
        unreadable: Vec::new(),
    })
}

fn native_integrity() -> CapabilityIntegrity {
    CapabilityIntegrity::NativeVerified {
        verifier_id: "member-metadata-checksum-v1".into(),
        witness_sha256: "b".repeat(64),
    }
}

fn content_integrity() -> CapabilityIntegrity {
    CapabilityIntegrity::ContentVerified {
        manifest_sha256: digest(b"metadata-range-manifest-v1"),
        content_sha256: digest(&[0x5a; 512]),
    }
}

fn capability(integrity: CapabilityIntegrity) -> Result<CapabilityRef, CapabilityError> {
    CapabilityRef::new(
        "member-metadata-7".into(),
        "member-view-7".into(),
        CapabilityKind::MemberMetadata,
        1,
        integrity,
        metadata_coverage(),
    )
}

fn roundtrip_integrity(value: &CapabilityIntegrity) {
    let encoded = serde_json::to_string(value).unwrap();
    assert_eq!(
        serde_json::from_str::<CapabilityIntegrity>(&encoded).unwrap(),
        *value
    );
}

#[test]
fn integrity_levels_roundtrip_with_explicit_and_distinct_proof_fields() {
    for value in [
        CapabilityIntegrity::Observed,
        native_integrity(),
        content_integrity(),
    ] {
        value.validate().unwrap();
        roundtrip_integrity(&value);
    }
    assert_eq!(
        serde_json::to_value(CapabilityIntegrity::Observed).unwrap(),
        json!({"kind": "observed"})
    );
    assert_eq!(
        serde_json::to_value(native_integrity()).unwrap(),
        json!({
            "kind": "native_verified",
            "verifierId": "member-metadata-checksum-v1",
            "witnessSha256": "b".repeat(64)
        })
    );
    assert_eq!(
        serde_json::to_value(content_integrity()).unwrap(),
        json!({
            "kind": "content_verified",
            "manifestSha256": digest(b"metadata-range-manifest-v1"),
            "contentSha256": digest(&[0x5a; 512])
        })
    );
}

#[test]
fn metadata_native_verification_requires_no_whole_image_content_manifest() {
    let value = capability(native_integrity()).unwrap();
    value.validate().unwrap();
    assert!(value.coverage.is_complete().unwrap());
    let TargetCoverage::Bytes(coverage) = &value.coverage else {
        panic!("member metadata must retain its byte coverage");
    };
    assert_eq!(coverage.verified_bytes().unwrap(), 512);
    assert_eq!(coverage.target_size, 1024 * 1024);
    let encoded = serde_json::to_value(&value).unwrap();
    assert_eq!(encoded["integrity"]["kind"], "native_verified");
    assert!(encoded["integrity"].get("manifestSha256").is_none());
    assert!(encoded["integrity"].get("contentSha256").is_none());
    assert_eq!(
        serde_json::from_value::<CapabilityRef>(encoded).unwrap(),
        value
    );
}

#[test]
fn native_verified_empty_namespace_needs_eof_proof_and_no_content_manifest() {
    let coverage = TargetCoverage::Namespace(NamespaceCoverage {
        context: CoverageContext {
            view_id: "member-view-7".into(),
            native_namespace: "committed-tree-root-7".into(),
            version: VersionBoundary {
                adapter_id: "native-tree-adapter".into(),
                native_kind: "committed-tree-version".into(),
                native_value: "17".into(),
                witness_sha256: "d".repeat(64),
            },
        },
        root_id: "root-7".into(),
        query_sha256: "e".repeat(64),
        ordering_id: "native-tree-order-v1".into(),
        pages: vec![NamespacePage {
            input_cursor: None,
            first_ordinal: 0,
            object_count: 0,
            page_sha256: "f".repeat(64),
            verified: true,
            end: NamespacePageEnd::Eof {
                witness_sha256: "a".repeat(64),
            },
        }],
    });
    let value = CapabilityRef::new(
        "committed-tree-7".into(),
        "member-view-7".into(),
        CapabilityKind::CommittedTree,
        1,
        native_integrity(),
        coverage.clone(),
    )
    .unwrap();
    assert!(value.coverage.is_complete().unwrap());
    assert_eq!(
        serde_json::from_value::<CapabilityRef>(serde_json::to_value(&value).unwrap()).unwrap(),
        value
    );
    assert!(matches!(
        CapabilityRef::new(
            value.target_id.clone(),
            value.view_id.clone(),
            value.kind,
            value.revision,
            CapabilityIntegrity::Observed,
            coverage,
        ),
        Err(CapabilityError::UnverifiedCoverage)
    ));
}

#[test]
fn observed_integrity_allows_only_unproven_classified_metadata() {
    assert!(matches!(
        capability(CapabilityIntegrity::Observed),
        Err(CapabilityError::UnverifiedCoverage)
    ));
    let TargetCoverage::Bytes(mut coverage) = metadata_coverage() else {
        panic!("member metadata must retain its byte coverage");
    };
    coverage.unknown = coverage.verified.clone();
    coverage.verified.clear();
    let value = CapabilityRef::new(
        "member-metadata-7".into(),
        "member-view-7".into(),
        CapabilityKind::MemberMetadata,
        1,
        CapabilityIntegrity::Observed,
        TargetCoverage::Bytes(coverage.clone()),
    )
    .unwrap();
    assert!(!value.coverage.has_proven_portion().unwrap());
    assert!(!value.coverage.is_complete().unwrap());
    assert_eq!(
        serde_json::from_value::<CapabilityRef>(serde_json::to_value(&value).unwrap()).unwrap(),
        value
    );
    coverage.native_holes = coverage.unknown.clone();
    coverage.unknown.clear();
    assert!(matches!(
        CapabilityRef::new(
            value.target_id,
            value.view_id,
            value.kind,
            value.revision,
            CapabilityIntegrity::Observed,
            TargetCoverage::Bytes(coverage),
        ),
        Err(CapabilityError::UnverifiedCoverage)
    ));
}

#[test]
fn native_proofs_require_a_named_verifier_and_a_lowercase_sha256_witness() {
    for verifier_id in ["", " \t\n"] {
        let integrity = CapabilityIntegrity::NativeVerified {
            verifier_id: verifier_id.into(),
            witness_sha256: "a".repeat(64),
        };
        assert!(matches!(
            integrity.validate(),
            Err(CapabilityError::Coverage(CoverageError::EmptyField {
                field: "verifier_id"
            }))
        ));
        assert!(capability(integrity.clone()).is_err());
        assert!(serde_json::from_value::<CapabilityIntegrity>(
            serde_json::to_value(integrity).unwrap()
        )
        .is_err());
    }
    for witness_sha256 in ["".into(), "a".repeat(63), "A".repeat(64), "g".repeat(64)] {
        let integrity = CapabilityIntegrity::NativeVerified {
            verifier_id: "native-checksum-v1".into(),
            witness_sha256,
        };
        assert!(matches!(
            integrity.validate(),
            Err(CapabilityError::Coverage(CoverageError::InvalidDigest {
                field: "native_witness_sha256"
            }))
        ));
        assert!(capability(integrity.clone()).is_err());
        assert!(serde_json::from_value::<CapabilityIntegrity>(
            serde_json::to_value(integrity).unwrap()
        )
        .is_err());
    }
}

#[test]
fn content_proofs_require_lowercase_sha256_manifest_and_content_digests() {
    for manifest_sha256 in ["".into(), "a".repeat(63), "A".repeat(64), "g".repeat(64)] {
        let integrity = CapabilityIntegrity::ContentVerified {
            manifest_sha256,
            content_sha256: digest(&[0x5a; 512]),
        };
        assert!(matches!(
            integrity.validate(),
            Err(CapabilityError::Coverage(CoverageError::InvalidDigest {
                field: "content_manifest_sha256"
            }))
        ));
        assert!(capability(integrity.clone()).is_err());
        assert!(serde_json::from_value::<CapabilityIntegrity>(
            serde_json::to_value(integrity).unwrap()
        )
        .is_err());
    }
    for content_sha256 in ["".into(), "a".repeat(63), "A".repeat(64), "g".repeat(64)] {
        let integrity = CapabilityIntegrity::ContentVerified {
            manifest_sha256: digest(b"metadata-range-manifest-v1"),
            content_sha256,
        };
        assert!(matches!(
            integrity.validate(),
            Err(CapabilityError::Coverage(CoverageError::InvalidDigest {
                field: "content_sha256"
            }))
        ));
        assert!(capability(integrity.clone()).is_err());
        assert!(serde_json::from_value::<CapabilityIntegrity>(
            serde_json::to_value(integrity).unwrap()
        )
        .is_err());
    }
}

#[test]
fn capability_identity_binds_each_native_and_content_proof_field() {
    let original = capability(native_integrity()).unwrap();
    for integrity in [
        CapabilityIntegrity::NativeVerified {
            verifier_id: "different-native-verifier-v1".into(),
            witness_sha256: "b".repeat(64),
        },
        CapabilityIntegrity::NativeVerified {
            verifier_id: "member-metadata-checksum-v1".into(),
            witness_sha256: "d".repeat(64),
        },
        content_integrity(),
    ] {
        let changed = capability(integrity.clone()).unwrap();
        assert_ne!(original.id, changed.id);
        let tampered = CapabilityRef {
            integrity,
            ..original.clone()
        };
        assert!(matches!(
            tampered.validate(),
            Err(CapabilityError::IdentityMismatch)
        ));
        assert!(
            serde_json::from_value::<CapabilityRef>(serde_json::to_value(tampered).unwrap())
                .is_err()
        );
    }
    let original = capability(content_integrity()).unwrap();
    for integrity in [
        CapabilityIntegrity::ContentVerified {
            manifest_sha256: digest(b"metadata-range-manifest-v2"),
            content_sha256: digest(&[0x5a; 512]),
        },
        CapabilityIntegrity::ContentVerified {
            manifest_sha256: digest(b"metadata-range-manifest-v1"),
            content_sha256: digest(&[0x5b; 512]),
        },
    ] {
        assert_ne!(original.id, capability(integrity).unwrap().id);
    }
}

#[test]
fn integrity_deserialization_requires_all_proof_fields_and_rejects_unknown_fields() {
    for value in [
        json!({"kind": "observed", "witnessSha256": "a".repeat(64)}),
        json!({"kind": "native_verified", "verifierId": "native-checksum-v1"}),
        json!({"kind": "native_verified", "witnessSha256": "a".repeat(64)}),
        json!({
            "kind": "native_verified",
            "verifierId": "native-checksum-v1",
            "witnessSha256": "a".repeat(64),
            "manifestSha256": "b".repeat(64)
        }),
        json!({
            "kind": "content_verified",
            "manifestSha256": digest(b"metadata-range-manifest-v1")
        }),
        json!({
            "kind": "content_verified",
            "contentSha256": digest(&[0x5a; 512])
        }),
        json!({
            "kind": "content_verified",
            "manifestSha256": digest(b"metadata-range-manifest-v1"),
            "contentSha256": digest(&[0x5a; 512]),
            "verifierId": "native-checksum-v1"
        }),
    ] {
        assert!(serde_json::from_value::<CapabilityIntegrity>(value).is_err());
    }
}

#[test]
fn content_capability_identity_uses_only_immutable_digest_fields() {
    let first = capability(content_integrity()).unwrap();
    let encoded = serde_json::to_value(&first).unwrap();
    let integrity = encoded["integrity"].as_object().unwrap();
    assert_eq!(integrity.len(), 3);
    assert!(integrity.contains_key("kind"));
    assert!(integrity.contains_key("manifestSha256"));
    assert!(integrity.contains_key("contentSha256"));
    let decoded_integrity =
        serde_json::from_value::<CapabilityIntegrity>(encoded["integrity"].clone()).unwrap();
    let independent = capability(decoded_integrity).unwrap();
    assert_eq!(first.id, independent.id);
    assert_eq!(first, independent);
    assert_eq!(
        serde_json::from_value::<CapabilityRef>(encoded).unwrap(),
        first
    );
}

#[test]
fn content_capability_rejects_receipt_and_execution_identity_fields() {
    let original = capability(content_integrity()).unwrap();
    for (field, value) in [
        ("receiptId", "receipt-for-attempt-1"),
        ("attemptId", "attempt-1"),
        ("owner", "owner-1"),
        ("runId", "run-1"),
    ] {
        let mut encoded = serde_json::to_value(&original).unwrap();
        encoded["integrity"][field] = json!(value);
        assert!(
            serde_json::from_value::<CapabilityIntegrity>(encoded["integrity"].clone()).is_err()
        );
        assert!(serde_json::from_value::<CapabilityRef>(encoded).is_err());
    }
    assert!(serde_json::from_value::<CapabilityIntegrity>(json!({
        "kind": "content_verified",
        "manifestSha256": digest(b"metadata-range-manifest-v1"),
        "receiptId": "receipt-for-attempt-1"
    }))
    .is_err());
}
