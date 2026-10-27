use super::*;
use serde_json::json;

#[test]
fn authorization_types_round_trip_and_reject_unknown_fields() {
    let request: McpHostCreateClientRequestDto = serde_json::from_value(json!({
        "name":"Investigation", "expiresAfterHours":24, "readEvidence":false, "allowedDataSourceIds":[],
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
        allowed_data_source_ids: vec![],
        export_grants: vec![],
    };
    let record = McpHostCredentialRecordDto {
        client: client.clone(),
        export_grants: vec![],
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

#[test]
fn authorization_records_migrate_legacy_embedded_export_paths() {
    let legacy = json!({
        "client": {
            "id": "client-id",
            "name": "Investigation",
            "caseId": "case-id",
            "caseSessionId": "session-id",
            "expiresAt": 123,
            "readEvidence": false,
            "allowedDataSourceIds": [],
            "exportGrants": [{"grantId":"grant-id", "displayName":"exports", "rootPath":"C:\\exports"}]
        },
        "tokenDigest": [3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
                        3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3]
    });
    let restored: McpHostCredentialRecordDto = serde_json::from_value(legacy).unwrap();
    assert_eq!(restored.client.export_grants[0].grant_id, "grant-id");
    assert_eq!(restored.export_grants[0].root_path, r"C:\exports");
}
