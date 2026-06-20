//! Normalized cloud audit entry schema.
//!
//! Provides a common representation for audit log entries across
//! AWS CloudTrail, Azure Activity Log, GCP Audit Log, and M365 Unified Audit Log.

use serde::{Deserialize, Serialize};

/// The cloud provider source of an audit entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CloudAuditSource {
    Aws,
    Azure,
    Gcp,
    M365,
}

/// A normalized cloud audit log entry.
///
/// Each provider parser normalizes its native record into this common shape,
/// preserving original provider-specific data in the `raw` field.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CloudAuditEntry {
    /// Which cloud provider produced this entry.
    pub source: CloudAuditSource,
    /// The action performed (e.g., "s3:PutObject", "Microsoft.Storage/storageAccounts/read").
    pub action: String,
    /// The principal (user, service account, or application) that performed the action.
    pub principal: Option<String>,
    /// The target resource affected by the action (ARN, resource ID, URL).
    pub target: Option<String>,
    /// ISO 8601 timestamp of the event.
    pub timestamp: Option<String>,
    /// Raw provider-specific payload as a JSON string.
    pub raw: Option<String>,
}

#[cfg(test)]
mod tests {
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
}
