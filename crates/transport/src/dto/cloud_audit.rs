use serde::{Deserialize, Serialize};

/// The cloud provider source of an audit entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum CloudAuditSourceDto {
    Aws,
    Azure,
    Gcp,
    M365,
}

/// A normalized cloud audit log entry across AWS, Azure, GCP, and M365.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CloudAuditEntryDto {
    /// Which cloud provider produced this entry.
    pub source: CloudAuditSourceDto,
    /// The action performed (e.g., "s3:PutObject", "Microsoft.Storage/storageAccounts/read").
    pub action: String,
    /// The principal (user, service account, or application) that performed the action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub principal: Option<String>,
    /// The target resource affected by the action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// ISO 8601 timestamp of the event.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Raw provider-specific payload as JSON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
}
