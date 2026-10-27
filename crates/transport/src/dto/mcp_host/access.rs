use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpHostExportGrantDto {
    pub grant_id: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpHostExportGrantRecordDto {
    pub grant_id: String,
    pub display_name: String,
    pub root_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpHostClientDto {
    pub id: String,
    pub name: String,
    pub case_id: String,
    pub case_session_id: String,
    pub expires_at: i64,
    #[serde(default = "default_read_evidence")]
    pub read_evidence: bool,
    #[serde(default)]
    pub allowed_data_source_ids: Vec<String>,
    #[serde(default)]
    pub export_grants: Vec<McpHostExportGrantDto>,
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
    #[serde(default = "default_read_evidence")]
    pub read_evidence: bool,
    #[serde(default)]
    pub allowed_data_source_ids: Vec<String>,
}

fn default_read_evidence() -> bool {
    true
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpHostCredentialRecordDto {
    pub client: McpHostClientDto,
    #[serde(default)]
    pub export_grants: Vec<McpHostExportGrantRecordDto>,
    pub token_digest: [u8; 32],
}

impl<'de> Deserialize<'de> for McpHostCredentialRecordDto {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct LegacyClientDto {
            id: String,
            name: String,
            case_id: String,
            case_session_id: String,
            expires_at: i64,
            #[serde(default = "default_read_evidence")]
            read_evidence: bool,
            #[serde(default)]
            allowed_data_source_ids: Vec<String>,
            #[serde(default)]
            export_grants: Vec<McpHostExportGrantRecordDto>,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct LegacyRecordDto {
            client: LegacyClientDto,
            token_digest: [u8; 32],
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct CurrentRecordDto {
            client: McpHostClientDto,
            #[serde(default)]
            export_grants: Vec<McpHostExportGrantRecordDto>,
            token_digest: [u8; 32],
        }

        let value = serde_json::Value::deserialize(deserializer)?;
        if let Ok(record) = serde_json::from_value::<CurrentRecordDto>(value.clone()) {
            return Ok(Self {
                client: record.client,
                export_grants: record.export_grants,
                token_digest: record.token_digest,
            });
        }
        let record =
            serde_json::from_value::<LegacyRecordDto>(value).map_err(serde::de::Error::custom)?;
        let client = McpHostClientDto {
            id: record.client.id,
            name: record.client.name,
            case_id: record.client.case_id,
            case_session_id: record.client.case_session_id,
            expires_at: record.client.expires_at,
            read_evidence: record.client.read_evidence,
            allowed_data_source_ids: record.client.allowed_data_source_ids,
            export_grants: record
                .client
                .export_grants
                .iter()
                .map(|grant| McpHostExportGrantDto {
                    grant_id: grant.grant_id.clone(),
                    display_name: grant.display_name.clone(),
                })
                .collect(),
        };
        Ok(Self {
            client,
            export_grants: record.client.export_grants,
            token_digest: record.token_digest,
        })
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/dto/mcp_host_access.rs"]
mod tests;
