use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpHostClientDto {
    pub id: String,
    pub name: String,
    pub case_id: String,
    pub case_session_id: String,
    pub expires_at: i64,
    pub read_evidence: bool,
    #[serde(default)]
    pub allowed_data_source_ids: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpHostCreatedClientDto {
    pub client: McpHostClientDto,
    pub token: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpHostCreateClientRequestDto {
    pub name: String,
    pub expires_after_hours: u32,
    pub read_evidence: bool,
    #[serde(default)]
    pub allowed_data_source_ids: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpHostCredentialRecordDto {
    pub client: McpHostClientDto,
    pub token_digest: [u8; 32],
}

#[cfg(test)]
#[path = "../../../tests/unit/dto/mcp_host_access.rs"]
mod tests;
