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
    pub took_ms: u64,
    pub items: Vec<SearchHitDto>,
}
