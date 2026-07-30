use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHighlightDto {
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchSnippetDto {
    pub text: String,
    pub highlights: Vec<SearchHighlightDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHitDto {
    pub file_id: String,
    pub path: String,
    pub score: f64,
    pub snippets: Vec<SearchSnippetDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResultPageDto {
    pub total: u64,
    pub available: u64,
    pub truncated: bool,
    pub took_ms: u64,
    pub items: Vec<SearchHitDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFileHitDto {
    pub file_id: String,
    pub data_source_id: String,
    pub data_source_name: String,
    pub name: String,
    pub path: String,
    pub entry_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
    pub deleted: bool,
    pub hidden: bool,
    pub system: bool,
    pub encrypted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SearchCoverageDto {
    pub ready_source_count: u32,
    pub indexed_source_count: u32,
    pub expected_entry_count: u64,
    pub indexed_entry_count: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_source_ids: Vec<String>,
    pub complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFileResultPageDto {
    pub total: u64,
    pub available: u64,
    pub truncated: bool,
    pub took_ms: u64,
    pub items: Vec<SearchFileHitDto>,
    pub coverage: SearchCoverageDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/search.rs"]
mod tests;
