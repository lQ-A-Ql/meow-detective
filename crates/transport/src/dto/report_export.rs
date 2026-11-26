use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ReportFormatDto {
    Html,
    Csv,
    Json,
    Markdown,
}

impl ReportFormatDto {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Csv => "csv",
            Self::Json => "json",
            Self::Markdown => "md",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ReportStatusDto {
    Running,
    Completed,
    Failed,
    Cancelled,
}
