//! Serde round-trip tests for the plugin analysis DTOs.

use super::*;

fn sample_module() -> PluginModuleDto {
    PluginModuleDto {
        plugin_id: "meow.plugin.prefetch".to_string(),
        display_name: "Prefetch".to_string(),
        plugin_version: "0.1.0".to_string(),
        evidence_platform: "windows".to_string(),
        families: vec![PluginFamilyCountDto {
            family: "Prefetch".to_string(),
            count: 3,
        }],
        total_count: 3,
        warnings: vec!["one warning".to_string()],
    }
}

#[test]
fn plugin_module_round_trip_uses_camel_case() {
    let module = sample_module();
    let json = serde_json::to_value(&module).unwrap();
    assert!(json.get("pluginId").is_some());
    assert!(json.get("displayName").is_some());
    assert!(json.get("pluginVersion").is_some());
    assert!(json.get("evidencePlatform").is_some());
    assert!(json.get("totalCount").is_some());
    assert_eq!(json["families"][0]["count"], 3);
    let back: PluginModuleDto = serde_json::from_value(json).unwrap();
    assert_eq!(back.plugin_id, "meow.plugin.prefetch");
}

#[test]
fn plugin_family_entries_round_trip_and_option_skip() {
    let mut attrs = serde_json::Map::new();
    attrs.insert("runCount".to_string(), serde_json::json!(3));
    let entries = PluginFamilyEntriesDto {
        plugin_id: "meow.plugin.prefetch".to_string(),
        family: "Prefetch".to_string(),
        total_count: 1,
        truncated: false,
        entries: vec![PluginArtifactEntryDto {
            artifact_id: "a1".to_string(),
            file_id: "ds:1:1".to_string(),
            source_path: "[P0]/Windows/Prefetch/EVIL.EXE-12345678.pf".to_string(),
            title: "EVIL.EXE".to_string(),
            summary: "run_count=3".to_string(),
            confidence: None,
            attrs,
            created_at: "2026-08-13T00:00:00Z".to_string(),
        }],
    };
    let json = serde_json::to_value(&entries).unwrap();
    // confidence is None and must be omitted.
    assert!(json["entries"][0].get("confidence").is_none());
    assert_eq!(json["entries"][0]["attrs"]["runCount"], 3);
    let back: PluginFamilyEntriesDto = serde_json::from_value(json).unwrap();
    assert_eq!(back.entries.len(), 1);
    assert!(!back.truncated);
}

#[test]
fn plugin_action_descriptor_round_trip_and_option_skip() {
    let descriptor = PluginActionDescriptorDto {
        id: "recoverKeys".to_string(),
        label: "从内存镜像恢复数据库密钥".to_string(),
        description: None,
        input_kind: "file".to_string(),
    };
    let json = serde_json::to_value(&descriptor).unwrap();
    assert_eq!(json["id"], "recoverKeys");
    assert_eq!(json["inputKind"], "file");
    // description is None and must be omitted.
    assert!(json.get("description").is_none());
    let back: PluginActionDescriptorDto = serde_json::from_value(json).unwrap();
    assert_eq!(back.id, "recoverKeys");
    assert_eq!(back.input_kind, "file");
}

#[test]
fn wechat_key_recovery_result_round_trip_carries_no_keys() {
    let result = WeChatKeyRecoveryResultDto {
        candidates_seen: 7,
        recovered_count: 1,
        matched_db_names: vec!["message_0.db".to_string()],
        unmatched_db_names: vec!["contact.db".to_string()],
    };
    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["candidatesSeen"], 7);
    assert_eq!(json["recoveredCount"], 1);
    assert_eq!(json["matchedDbNames"][0], "message_0.db");
    assert_eq!(json["unmatchedDbNames"][0], "contact.db");
    // The DTO must never carry key material.
    assert!(json.get("keys").is_none());
    let back: WeChatKeyRecoveryResultDto = serde_json::from_value(json).unwrap();
    assert_eq!(back.recovered_count, 1);
}
