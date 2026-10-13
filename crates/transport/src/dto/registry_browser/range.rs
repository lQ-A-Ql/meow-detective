use serde::{Deserialize, Serialize};

use super::RegistryBrowserDiagnosticDto;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryValueRangeRequestDto {
    pub session_id: String,
    pub key_id: String,
    pub value_id: String,
    pub offset: u64,
    pub length: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RegistryValueDecodeStatusDto {
    Decoded,
    RawOnly,
    Partial,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryValueRangeDto {
    pub value_id: String,
    /// All offsets address the assembled value payload, not a contiguous disk range.
    pub offset: u64,
    pub total_length: u64,
    pub bytes: Vec<u8>,
    pub eof: bool,
    pub decode_status: RegistryValueDecodeStatusDto,
    pub diagnostics: Vec<RegistryBrowserDiagnosticDto>,
    /// Integers are strings, including QWORD values larger than 2^53.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decoded_text: Option<String>,
}
