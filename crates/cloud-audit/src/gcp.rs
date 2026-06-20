//! GCP Audit Log JSON parser.
//!
//! Parses Google Cloud Audit Log records (JSON format) and normalizes them into
//! `CloudAuditEntry` structs.
//!
//! GCP Audit Log record key fields (from protoPayload):
//! - `serviceName` — GCP service (e.g., "storage.googleapis.com")
//! - `methodName` — API method (e.g., "storage.objects.get")
//! - `resourceName` — affected resource
//! - `authenticationInfo.principalEmail` — caller identity
//! - `timestamp` — ISO 8601 timestamp
//! - `request` / `response` — request/response metadata
//! - `status` — status of the operation
//!
//! The file format is JSON Lines (one log entry per line is common) or
//! a JSON array.

use super::normalize::{CloudAuditEntry, CloudAuditSource};
use serde::{Deserialize, Serialize};

/// Raw parsed GCP Audit Log entry (subset of fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GcpAuditLogRecord {
    #[serde(default)]
    pub proto_payload: Option<GcpProtoPayload>,
    #[serde(default)]
    pub resource: Option<serde_json::Value>,
    #[serde(default)]
    pub timestamp: Option<String>,
}

/// The `protoPayload` field within a GCP audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GcpProtoPayload {
    #[serde(rename = "@type", default)]
    pub type_url: Option<String>,
    #[serde(default)]
    pub service_name: Option<String>,
    #[serde(default)]
    pub method_name: Option<String>,
    #[serde(default)]
    pub resource_name: Option<String>,
    #[serde(default)]
    pub authentication_info: Option<GcpAuthInfo>,
}

/// Authentication info within a GCP audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GcpAuthInfo {
    #[serde(default)]
    pub principal_email: Option<String>,
    #[serde(default)]
    pub principal_subject: Option<String>,
}

/// Parse a GCP Audit Log file (JSON Lines or JSON array).
pub fn parse_gcp_audit_log(data: &str) -> Result<Vec<CloudAuditEntry>, String> {
    if data.trim().is_empty() {
        return Err("GCP Audit Log data is empty".to_string());
    }

    let mut entries: Vec<CloudAuditEntry> = Vec::new();

    let trimmed = data.trim();
    let raw_values: Vec<serde_json::Value> = if trimmed.starts_with('[') {
        serde_json::from_str::<Vec<serde_json::Value>>(trimmed)
            .map_err(|e| format!("Invalid GCP Audit Log JSON array: {}", e))?
    } else if trimmed.starts_with('{') {
        // Could be a single object or JSON Lines
        // Check if it's a wrapped response (some tools output {"entries": [...]})
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(list) = val.get("entries").and_then(|v| v.as_array()) {
                list.clone()
            } else {
                vec![val]
            }
        } else {
            // JSON Lines
            let mut vals = Vec::new();
            for line in trimmed.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let v: serde_json::Value = serde_json::from_str(line)
                    .map_err(|e| format!("Invalid GCP Audit Log JSON line: {}", e))?;
                vals.push(v);
            }
            vals
        }
    } else {
        return Err("GCP Audit Log does not appear to be valid JSON".to_string());
    };

    for value in &raw_values {
        if let Ok(record) = serde_json::from_value::<GcpAuditLogRecord>(value.clone()) {
            entries.push(normalize_gcp(&record, value));
        }
    }

    if entries.is_empty() {
        return Err("No GCP Audit Log records found in data".to_string());
    }

    Ok(entries)
}

fn normalize_gcp(record: &GcpAuditLogRecord, raw: &serde_json::Value) -> CloudAuditEntry {
    let svc = record
        .proto_payload
        .as_ref()
        .and_then(|pp| pp.service_name.as_deref())
        .unwrap_or("unknown.googleapis.com")
        .strip_suffix(".googleapis.com")
        .unwrap_or("unknown");

    let method = record
        .proto_payload
        .as_ref()
        .and_then(|pp| pp.method_name.as_deref())
        .unwrap_or("Unknown");

    let action = format!("{}.{}", svc, method);

    let principal = record
        .proto_payload
        .as_ref()
        .and_then(|pp| pp.authentication_info.as_ref())
        .and_then(|auth| auth.principal_email.clone());

    let target = record
        .proto_payload
        .as_ref()
        .and_then(|pp| pp.resource_name.clone());

    let raw_json = serde_json::to_string(raw).ok();

    CloudAuditEntry {
        source: CloudAuditSource::Gcp,
        action,
        principal,
        target,
        timestamp: record.timestamp.clone(),
        raw: raw_json,
    }
}

#[cfg(test)]
mod tests {
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
}
