use crate::commands::CalculateEvidenceDigestRequest;
use crate::dto::{DigestAlgorithmDto, DigestScopeDto};

#[test]
fn file_scope_requires_file_id() {
    let request = CalculateEvidenceDigestRequest {
        scope: DigestScopeDto::File,
        algorithm: DigestAlgorithmDto::Sha256,
        data_source_id: None,
        file_id: None,
        partition_index: None,
        force_recompute: false,
    };
    assert!(request.validate().is_err());
}

#[test]
fn partition_scope_requires_index_and_source() {
    let request = CalculateEvidenceDigestRequest {
        scope: DigestScopeDto::Partition,
        algorithm: DigestAlgorithmDto::Sha256,
        data_source_id: Some("source".into()),
        file_id: None,
        partition_index: None,
        force_recompute: false,
    };
    assert!(request.validate().is_err());
}

#[test]
fn force_recompute_is_optional_and_serializes_when_enabled() {
    let mut request = CalculateEvidenceDigestRequest {
        scope: DigestScopeDto::LogicalDisk,
        algorithm: DigestAlgorithmDto::Sha256,
        data_source_id: Some("source".into()),
        file_id: None,
        partition_index: None,
        force_recompute: false,
    };
    let default_json = serde_json::to_value(&request).unwrap();
    assert!(default_json.get("forceRecompute").is_none());

    request.force_recompute = true;
    let forced_json = serde_json::to_value(request).unwrap();
    assert_eq!(forced_json["forceRecompute"], true);
}
