use serde::{Deserialize, Serialize};

/// Wire-format mirror of `RegistryTransactionOperation`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RegistryTransactionOperationDto {
    CreateKey,
    DeleteKey,
    SetValue,
    DeleteValue,
    RenameKey,
}

/// A single transaction-log entry transported to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryTransactionDto {
    pub operation: RegistryTransactionOperationDto,
    pub key_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_after: Option<String>,
    pub sequence_number: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

/// Parsed result of a transaction log file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxLogParseResultDto {
    pub transactions: Vec<RegistryTransactionDto>,
    pub primary: bool,
    pub warnings: Vec<String>,
}

/// A single auto-start entry from a Run / RunOnce key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryRunKeyDto {
    pub key_path: String,
    pub value_name: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

/// A single entry from the Explorer RecentDocs MRU.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecentDocDto {
    pub file_name: String,
    pub extension: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_accessed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lnk_target: Option<String>,
}

/// A single UserAssist program-execution tracking entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UserAssistEntryDto {
    pub executable: String,
    pub run_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run: Option<String>,
    pub focus_time_ms: u64,
}

/// A mount-point entry from Explorer MountPoints2.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MountPointDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drive_letter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_guid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_mounted: Option<String>,
}

/// Aggregated NTUSER.DAT extraction result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NtuserInfoDto {
    pub run_keys: Vec<RegistryRunKeyDto>,
    pub recent_docs: Vec<RecentDocDto>,
    pub user_assist: Vec<UserAssistEntryDto>,
    pub typed_urls: Vec<String>,
    pub word_wheel_query: Vec<String>,
    pub mount_points: Vec<MountPointDto>,
    pub warnings: Vec<String>,
}

/// A local user account extracted from a SAM registry hive.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SamUserDto {
    pub username: String,
    pub rid: u32,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub full_name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub comment: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub home_dir: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub profile_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_login: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_last_set: Option<String>,
    pub account_disabled: bool,
    pub account_locked: bool,
    pub admin_count: u32,
    pub group_memberships: Vec<String>,
    /// NTLM hash in "lm_hash:nt_hash" format; only present when extracted from SAM+SYSTEM BootKey.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_hash: Option<String>,
    /// Hash type present: "NTLM", "LM", "Both", or absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_hash_type: Option<String>,
}

/// A local group extracted from a SAM registry hive.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SamGroupDto {
    pub name: String,
    pub rid: u32,
    pub members: Vec<String>,
}

/// Domain-level password policy extracted from SAM DomainAccountF.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SamPasswordPolicyDto {
    /// Maximum password age in days (0 = never expires).
    pub max_password_age_days: u64,
    /// Minimum password age in days (0 = can change immediately).
    pub min_password_age_days: u64,
    /// Minimum password length in characters.
    pub min_password_length: u16,
    /// Number of passwords remembered in history.
    pub password_history_length: u16,
    /// Number of invalid attempts before account lockout (0 = never lock).
    pub lockout_threshold: u16,
    /// Account lockout duration in minutes (0 = locked until administrator resets).
    pub lockout_duration_minutes: u64,
    /// Lockout observation window in minutes.
    pub lockout_observation_window_minutes: u64,
}

/// Aggregated SAM hive extraction result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamInfoDto {
    pub users: Vec<SamUserDto>,
    pub groups: Vec<SamGroupDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_policy: Option<SamPasswordPolicyDto>,
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_transaction_dto_serializes_as_camel_case() {
        let dto = RegistryTransactionDto {
            operation: RegistryTransactionOperationDto::SetValue,
            key_path: "\\Registry\\Machine\\SOFTWARE\\Test".to_string(),
            value_name: Some("KeyName".to_string()),
            data_before: Some("aGV4".to_string()),
            data_after: Some("d29ybGQ=".to_string()),
            sequence_number: 42,
            timestamp: Some("2026-06-14T12:00:00Z".to_string()),
        };

        let value = serde_json::to_value(&dto).unwrap();
        assert_eq!(value["operation"], "setValue");
        assert_eq!(value["keyPath"], "\\Registry\\Machine\\SOFTWARE\\Test");
        assert_eq!(value["valueName"], "KeyName");
        assert_eq!(value["sequenceNumber"], 42);
        assert_eq!(value["timestamp"], "2026-06-14T12:00:00Z");
        // Check that snake_case keys are absent.
        assert!(value.get("key_path").is_none());
        assert!(value.get("value_name").is_none());
        assert!(value.get("sequence_number").is_none());
    }

    #[test]
    fn registry_transaction_dto_skips_optional_fields() {
        let dto = RegistryTransactionDto {
            operation: RegistryTransactionOperationDto::CreateKey,
            key_path: "\\Key".to_string(),
            value_name: None,
            data_before: None,
            data_after: None,
            sequence_number: 1,
            timestamp: None,
        };

        let value = serde_json::to_value(&dto).unwrap();
        assert!(value.get("valueName").is_none());
        assert!(value.get("dataBefore").is_none());
        assert!(value.get("dataAfter").is_none());
        assert!(value.get("timestamp").is_none());
    }
}
