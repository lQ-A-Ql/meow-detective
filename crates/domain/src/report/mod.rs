use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ReportId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReportStatus {
    Draft,
    Generating,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportHistoryItem {
    pub id: ReportId,
    pub file_name: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub status: ReportStatus,
    pub progress: Option<u32>,
}
