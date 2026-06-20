use serde::{Deserialize, Serialize};

/// An Android contact from contacts2.db.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AndroidContactDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub phones: Vec<String>,
    pub emails: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
}

/// An Android SMS or MMS record from mmssms.db.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AndroidSmsDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// SMS type: 1 = received, 2 = sent, 3 = draft, etc.
    pub sms_type: i32,
}

/// An Android Chrome browsing history visit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AndroidChromeVisitDto {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visit_time: Option<String>,
}

/// An Android call log record from calllog.db (contacts2.db calls table).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AndroidCallDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Call type: 1 = incoming, 2 = outgoing, 3 = missed
    pub call_type: i32,
}

/// Metadata extracted from an ADB .ab backup file header.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AndroidBackupDto {
    /// The backup format version extracted from the header.
    pub version: u32,
    /// Whether the backup payload is zlib-compressed.
    pub is_compressed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encryption_algorithm: Option<String>,
}
