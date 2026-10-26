use super::*;
use serde_json::json;

#[test]
fn authorization_types_round_trip_and_reject_unknown_fields() {
    let request: McpHostCreateClientRequestDto = serde_json::from_value(json!({
        "name":"Investigation", "expiresAfterHours":24, "readEvidence":false,
    }))
    .unwrap();
    let value = serde_json::to_value(request).unwrap();
    assert_eq!(value["expiresAfterHours"], 24);
    assert!(
        serde_json::from_value::<McpHostCreateClientRequestDto>(json!({
            "name":"Investigation", "expiresAfterHours":24, "readEvidence":false, "admin":true,
        }))
        .is_err()
    );
    let client = McpHostClientDto {
        id: "client-id".into(),
        name: "Investigation".into(),
        case_id: "case-id".into(),
        case_session_id: "session-id".into(),
        expires_at: 123,
        read_evidence: false,
    };
    let record = McpHostCredentialRecordDto {
        client: client.clone(),
        token_digest: [3; 32],
    };
    let restored: McpHostCredentialRecordDto =
        serde_json::from_value(serde_json::to_value(record).unwrap()).unwrap();
    assert_eq!(restored.client, client);
    let created = McpHostCreatedClientDto {
        client,
        token: "one-time-value".into(),
    };
    let restored: McpHostCreatedClientDto =
        serde_json::from_value(serde_json::to_value(created).unwrap()).unwrap();
    assert_eq!(restored.token, "one-time-value");
}
