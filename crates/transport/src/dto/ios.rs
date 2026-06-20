use serde::{Deserialize, Serialize};

/// A file entry from an iOS backup Manifest.db.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IosBackupFileDto {
    pub file_id: String,
    pub domain: String,
    pub relative_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<i32>,
}

/// A parsed iOS contact (address book entry).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IosContactDto {
    pub name: String,
    pub phones: Vec<String>,
    pub emails: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
}

/// A parsed iOS SMS or iMessage record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IosMessageDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender: Option<String>,
    pub recipients: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    pub is_from_me: bool,
}

/// A parsed iOS photo (asset) record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IosPhotoDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    pub albums: Vec<String>,
}

/// A parsed iOS Safari browsing history entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IosSafariEntryDto {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visit_time: Option<String>,
    pub visit_count: i32,
}

/// A parsed iOS call log record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IosCallDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<i32>,
    pub is_outgoing: bool,
}

/// A parsed iOS Notes entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IosNoteDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
}
