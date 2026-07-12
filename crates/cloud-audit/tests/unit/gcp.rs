use super::*;

#[test]
fn parse_empty_data() {
    let result = parse_gcp_audit_log("");
    assert!(result.is_err());
}

#[test]
fn parse_gcp_json_array() {
    let json = r#"[
        {
            "protoPayload": {
                "@type": "type.googleapis.com/google.cloud.audit.AuditLog",
                "serviceName": "storage.googleapis.com",
                "methodName": "storage.objects.get",
                "resourceName": "projects/_/buckets/my-bucket/objects/key.txt",
                "authenticationInfo": {
                    "principalEmail": "alice@example.com"
                }
            },
            "timestamp": "2024-06-15T12:00:00Z"
        },
        {
            "protoPayload": {
                "serviceName": "compute.googleapis.com",
                "methodName": "v1.compute.instances.start",
                "resourceName": "projects/my-project/zones/us-central1-a/instances/instance-1",
                "authenticationInfo": {
                    "principalEmail": "bob@example.com"
                }
            },
            "timestamp": "2024-06-15T12:05:00Z"
        }
    ]"#;

    let entries = parse_gcp_audit_log(json).expect("should parse");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].action, "storage.storage.objects.get");
    assert_eq!(entries[0].principal.as_deref(), Some("alice@example.com"));
    assert_eq!(
        entries[0].target.as_deref(),
        Some("projects/_/buckets/my-bucket/objects/key.txt")
    );
    assert_eq!(entries[1].action, "compute.v1.compute.instances.start");
}

#[test]
fn parse_gcp_single_object() {
    let json = r#"{
        "protoPayload": {
            "serviceName": "iam.googleapis.com",
            "methodName": "google.iam.admin.v1.CreateServiceAccount",
            "resourceName": "projects/my-project/serviceAccounts/sa-1@my-project.iam.gserviceaccount.com",
            "authenticationInfo": {
                "principalEmail": "admin@example.com"
            }
        },
        "timestamp": "2024-06-15T13:00:00Z"
    }"#;

    let entries = parse_gcp_audit_log(json).expect("should parse");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].action,
        "iam.google.iam.admin.v1.CreateServiceAccount"
    );
}
