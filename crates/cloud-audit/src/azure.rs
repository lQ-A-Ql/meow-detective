//! Azure Activity Log JSON parser.
//!
//! Parses Azure Activity Log records (JSON format) and normalizes them into
//! `CloudAuditEntry` structs.
//!
//! Azure Activity Log record key fields:
//! - `authorization` — action + scope (e.g., "Microsoft.Storage/storageAccounts/read")
//! - `caller` — caller UPN or service principal ID
//! - `eventTimestamp` / `eventTime` — ISO 8601 timestamp
//! - `resourceId` — full resource ID path
//! - `resourceGroupName` — resource group
//! - `operationName` — operation name
//! - `category` — log category
//! - `properties` — additional event properties
//!
//! The file format is a JSON array or JSON Lines.

use super::normalize::{CloudAuditEntry, CloudAuditSource};
use serde::{Deserialize, Serialize};

/// Raw parsed Azure Activity Log record (subset of fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AzureActivityLogRecord {
    #[serde(default)]
    pub authorization: Option<serde_json::Value>,
    #[serde(default)]
    pub caller: Option<String>,
    #[serde(default, alias = "eventTime")]
    pub event_timestamp: Option<String>,
    #[serde(default)]
    pub resource_id: Option<String>,
    #[serde(default)]
    pub operation_name: Option<serde_json::Value>,
}

/// Parse an Azure Activity Log file (JSON array or JSON Lines).
pub fn parse_azure_activity_log(data: &str) -> Result<Vec<CloudAuditEntry>, String> {
    if data.trim().is_empty() {
        return Err("Azure Activity Log data is empty".to_string());
    }

    let mut entries: Vec<CloudAuditEntry> = Vec::new();

    let trimmed = data.trim();
    let raw_values: Vec<serde_json::Value> = if trimmed.starts_with('[') {
        // JSON array
        serde_json::from_str::<Vec<serde_json::Value>>(trimmed)
            .map_err(|e| format!("Invalid Azure Activity Log JSON array: {}", e))?
    } else if trimmed.starts_with('{') {
        // Try single object or JSON Lines
        // First try parsing the whole thing as one object
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
            // Check if it contains a value/records array
            if let Some(list) = val
                .get("value")
                .or_else(|| val.get("records"))
                .and_then(|v| v.as_array())
            {
                list.clone()
            } else {
                vec![val]
            }
        } else {
            // JSON Lines: one object per line
            let mut vals = Vec::new();
            for line in trimmed.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let v: serde_json::Value = serde_json::from_str(line)
                    .map_err(|e| format!("Invalid Azure Activity Log JSON line: {}", e))?;
                vals.push(v);
            }
            vals
        }
    } else {
        return Err("Azure Activity Log does not appear to be valid JSON".to_string());
    };

    for value in &raw_values {
        if let Ok(record) = serde_json::from_value::<AzureActivityLogRecord>(value.clone()) {
            entries.push(normalize_azure(&record, value));
        }
    }

    if entries.is_empty() {
        return Err("No Azure Activity Log records found in data".to_string());
    }

    Ok(entries)
}

fn normalize_azure(record: &AzureActivityLogRecord, raw: &serde_json::Value) -> CloudAuditEntry {
    let action = record
        .authorization
        .as_ref()
        .and_then(|auth| auth.get("action").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .or_else(|| {
            record
                .operation_name
                .as_ref()
                .and_then(|op| op.get("value").or_else(|| op.get("localizedValue")))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "Unknown".to_string());

    let target = record.resource_id.clone();

    let raw_json = serde_json::to_string(raw).ok();

    CloudAuditEntry {
        source: CloudAuditSource::Azure,
        action,
        principal: record.caller.clone(),
        target,
        timestamp: record.event_timestamp.clone(),
        raw: raw_json,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_data() {
        let result = parse_azure_activity_log("");
        assert!(result.is_err());
    }

    #[test]
    fn parse_azure_json_array() {
        let json = r#"[
            {
                "authorization": {
                    "action": "Microsoft.Storage/storageAccounts/read",
                    "scope": "/subscriptions/sub-id/resourceGroups/rg/providers/Microsoft.Storage/storageAccounts/mystorage"
                },
                "caller": "alice@example.com",
                "eventTimestamp": "2024-06-15T12:00:00Z",
                "resourceId": "/subscriptions/sub-id/resourceGroups/rg/providers/Microsoft.Storage/storageAccounts/mystorage"
            },
            {
                "authorization": {
                    "action": "Microsoft.Compute/virtualMachines/start/action"
                },
                "caller": "bob@example.com",
                "eventTimestamp": "2024-06-15T12:05:00Z",
                "resourceId": "/subscriptions/sub-id/resourceGroups/rg/providers/Microsoft.Compute/virtualMachines/myvm"
            }
        ]"#;

        let entries = parse_azure_activity_log(json).expect("should parse");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].action, "Microsoft.Storage/storageAccounts/read");
        assert_eq!(entries[0].principal.as_deref(), Some("alice@example.com"));
        assert!(entries[0].target.is_some());
        assert_eq!(
            entries[1].action,
            "Microsoft.Compute/virtualMachines/start/action"
        );
    }

    #[test]
    fn parse_azure_single_object() {
        let json = r#"{
            "authorization": {"action": "Microsoft.Network/networkSecurityGroups/read"},
            "caller": "sp-xyz",
            "eventTimestamp": "2024-06-15T13:00:00Z",
            "resourceId": "/subscriptions/sub-id/resourceGroups/rg/providers/Microsoft.Network/networkSecurityGroups/nsg1"
        }"#;

        let entries = parse_azure_activity_log(json).expect("should parse");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].principal.as_deref(), Some("sp-xyz"));
    }
}
