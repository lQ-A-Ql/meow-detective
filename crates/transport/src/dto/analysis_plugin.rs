use serde::{Deserialize, Serialize};

/// One plugin-produced analysis module under a data source (the "微信/QQ"
/// style app module in the analysis view).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginModuleDto {
    pub plugin_id: String,
    pub display_name: String,
    pub plugin_version: String,
    pub evidence_platform: String,
    pub families: Vec<PluginFamilyCountDto>,
    pub total_count: u64,
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// Artifact count of one declared family inside a plugin module.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginFamilyCountDto {
    pub family: String,
    pub count: u64,
}

/// One page of generic plugin artifact entries for a family.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginFamilyEntriesDto {
    pub plugin_id: String,
    pub family: String,
    pub total_count: u64,
    pub truncated: bool,
    pub entries: Vec<PluginArtifactEntryDto>,
}

/// Generic plugin artifact entry; `attrs` keys are camelCase by contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginArtifactEntryDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub title: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    pub attrs: serde_json::Map<String, serde_json::Value>,
    pub created_at: String,
}

/// One self-described plugin action (ABI doc §3 optional export): the
/// plugin's `describe` response element. `label` is user-facing (may be
/// Chinese); `inputKind` is `"file"` or `"none"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginActionDescriptorDto {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_kind: String,
}

/// One database key verified against that database's encrypted page 1.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeChatRecoveredKeyDto {
    pub database_name: String,
    pub key_hex: String,
}

/// Outcome of a WeChat database-key recovery run. `recoveredKeys` is
/// intentionally returned to the local investigator UI for plaintext display
/// in the plugin title. It must not be copied into logs, audit details,
/// artifacts, reports, or generic plugin metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeChatKeyRecoveryResultDto {
    pub candidates_seen: u64,
    pub recovered_count: u64,
    pub matched_db_names: Vec<String>,
    pub unmatched_db_names: Vec<String>,
    pub recovered_keys: Vec<WeChatRecoveredKeyDto>,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/analysis_plugin.rs"]
mod tests;
