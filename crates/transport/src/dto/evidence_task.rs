use super::{EvidenceDigestDto, NestedEvidenceMaterializedDto};
use crate::commands::{CalculateEvidenceDigestRequest, MaterializeNestedEvidenceRequest};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "request",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum EvidenceTaskRequestDto {
    CarvePng(super::PngCarvingRequestDto),
    Digest(CalculateEvidenceDigestRequest),
    NestedImport(MaterializeNestedEvidenceRequest),
    ContentIndex {
        data_source_id: String,
    },
    RegistrySearch {
        file_id: String,
        session_id: String,
        query: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "result", rename_all = "camelCase")]
pub enum EvidenceTaskResultDto {
    CarvePng(super::PngCarvingSummaryDto),
    Digest(EvidenceDigestDto),
    NestedImport(NestedEvidenceMaterializedDto),
    ContentIndex(ContentIndexSummaryDto),
    RegistrySearch(super::RegistrySearchPageDto),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ContentIndexSummaryDto {
    pub indexed: u64,
    pub binary: u64,
    pub oversized: u64,
    pub unreadable: u64,
    pub unsupported_encoding: u64,
    pub not_applicable: u64,
    pub expected: u64,
    pub complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentIndexCoverageDto {
    pub summary: ContentIndexSummaryDto,
    pub stale: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_revision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceTaskDto {
    pub task_id: String,
    pub kind: String,
    pub state: String,
    pub source_revision: String,
    pub bytes_processed: u64,
    pub total_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<EvidenceTaskResultDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}
