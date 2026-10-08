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
    };
    assert!(request.validate().is_err());
}
