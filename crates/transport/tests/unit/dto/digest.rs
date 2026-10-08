use crate::dto::{DigestAlgorithmDto, DigestScopeDto, DigestStatusDto, EvidenceDigestDto};

#[test]
fn evidence_digest_uses_camel_case_contract() {
    let digest = EvidenceDigestDto {
        scope: DigestScopeDto::ContainerFile,
        algorithm: DigestAlgorithmDto::Sha256,
        value: "a".repeat(64),
        byte_length: 12,
        status: DigestStatusDto::Completed,
    };
    let value = serde_json::to_value(digest).expect("serialize");
    assert_eq!(value["byteLength"], 12);
    assert_eq!(value["scope"], "containerFile");
    assert_eq!(value["algorithm"], "sha256");
    assert_eq!(value["status"], "completed");
}
