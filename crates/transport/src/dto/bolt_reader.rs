use serde::{Deserialize, Serialize};

pub const BOLT_READER_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoltReadLimitsDto {
    pub max_file_bytes: u64,
    pub max_entries: u32,
    pub max_depth: u32,
    pub max_output_bytes: u64,
    pub memory_limit_bytes: u64,
    pub max_runtime_millis: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoltReadRequestDto {
    pub protocol_version: u32,
    pub request_id: String,
    pub input_path: String,
    pub input_size: u64,
    pub input_sha256: String,
    pub prefix: Vec<Vec<u8>>,
    pub record_keys: Vec<Vec<u8>>,
    pub value_keys: Vec<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_depth: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bucket_parent_depth: Option<u32>,
    pub parent_suffix: Vec<Vec<u8>>,
    pub include_buckets: bool,
    pub limits: BoltReadLimitsDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoltReadEntryDto {
    pub bucket_path: Vec<Vec<u8>>,
    pub key: Vec<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_length: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Vec<u8>>,
    pub is_bucket: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoltReadReportDto {
    pub page_size: u32,
    pub txid: u64,
    pub metadata_checksum_verified: bool,
    pub entries: Vec<BoltReadEntryDto>,
    pub visited_entries: u64,
    pub visited_buckets: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing_bucket: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum BoltReadResultDto {
    Complete { report: BoltReadReportDto },
    Failed { code: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoltReadResponseDto {
    pub protocol_version: u32,
    pub request_id: String,
    pub result: BoltReadResultDto,
}
