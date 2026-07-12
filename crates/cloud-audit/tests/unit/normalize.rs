use super::*;

#[test]
fn cloud_audit_entry_serialization_roundtrip() {
    let entry = CloudAuditEntry {
        source: CloudAuditSource::Aws,
        action: "s3:PutObject".to_string(),
        principal: Some("arn:aws:iam::123456789012:user/alice".to_string()),
        target: Some("arn:aws:s3:::my-bucket/key.txt".to_string()),
        timestamp: Some("2024-06-15T12:00:00Z".to_string()),
        raw: Some(r#"{"eventVersion":"1.08"}"#.to_string()),
    };

    let json = serde_json::to_string(&entry).expect("serialize");
    let back: CloudAuditEntry = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(entry, back);
}

#[test]
fn cloud_audit_entry_minimal() {
    let entry = CloudAuditEntry {
        source: CloudAuditSource::Gcp,
        action: "storage.objects.get".to_string(),
        principal: None,
        target: None,
        timestamp: None,
        raw: None,
    };

    let json = serde_json::to_string(&entry).expect("serialize");
    let back: CloudAuditEntry = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(entry, back);
}
