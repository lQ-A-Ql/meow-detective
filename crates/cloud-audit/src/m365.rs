//! Microsoft 365 Unified Audit Log CSV parser.
//!
//! Parses M365 Unified Audit Log records (CSV format exported from the
//! Microsoft Purview compliance portal) and normalizes them into
//! `CloudAuditEntry` structs.
//!
//! Key CSV columns:
//! - `CreationDate` — ISO 8601 timestamp
//! - `UserIds` — UPN or user principal name
//! - `Operations` — operation name (e.g., "FileDownloaded", "UserLoggedIn")
//! - `AuditData` — JSON blob with detailed event data
//! - `Item` — affected item/resource
//! - `Workload` — Microsoft 365 service (e.g., "SharePoint", "Exchange")
//! - `Id` — unique record identifier
//!
//! The `AuditData` column contains a nested JSON object with fields like
//! `Operation`, `UserId`, `ClientIP`, `ObjectId`, `SourceFileName`, etc.

use super::normalize::{CloudAuditEntry, CloudAuditSource};
use csv::ReaderBuilder;

/// Parse an M365 Unified Audit Log CSV file.
pub fn parse_m365_audit_log(data: &str) -> Result<Vec<CloudAuditEntry>, String> {
    if data.trim().is_empty() {
        return Err("M365 audit log data is empty".to_string());
    }

    let mut reader = ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(data.as_bytes());

    let entries: Vec<CloudAuditEntry> = reader
        .records()
        .filter_map(|result| match result {
            Ok(record) => {
                let raw_json = record_to_json_value(&record);
                Some(normalize_m365(&record, &raw_json))
            }
            Err(_) => None,
        })
        .collect();

    if entries.is_empty() {
        return Err("No M365 audit log records found in data".to_string());
    }

    Ok(entries)
}

/// Convert a CSV record to a JSON value for the raw field.
fn record_to_json_value(record: &csv::StringRecord) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (i, field) in record.iter().enumerate() {
        let key = format!("col_{}", i);
        map.insert(key, serde_json::Value::String(field.to_string()));
    }
    serde_json::Value::Object(map)
}

fn normalize_m365(record: &csv::StringRecord, raw: &serde_json::Value) -> CloudAuditEntry {
    // Access columns by index. The standard M365 Unified Audit Log CSV export includes:
    // 0: CreationDate, 1: UserIds, 2: Operations, 3: AuditData, 4: Item, 5: Id, 6: Workload
    let create_date: Option<String> = record.get(0).map(|s| s.to_string());
    let user_ids: Option<String> = record.get(1).map(|s| s.to_string());
    let operations: Option<String> = record.get(2).map(|s| s.to_string());
    let audit_data: Option<String> = record.get(3).map(|s| s.to_string());

    // Try to parse AuditData JSON for enriched fields
    let (action_override, principal_override, target_override, timestamp_override) =
        if let Some(ref ad) = audit_data {
            match serde_json::from_str::<serde_json::Value>(ad) {
                Ok(val) => {
                    let action = val
                        .get("Operation")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let principal = val
                        .get("UserId")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let target = val
                        .get("ObjectId")
                        .or_else(|| val.get("SourceFileName"))
                        .or_else(|| val.get("TargetContextId"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let timestamp = val
                        .get("CreationTime")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    (action, principal, target, timestamp)
                }
                Err(_) => (None, None, None, None),
            }
        } else {
            (None, None, None, None)
        };

    let action = action_override
        .or(operations)
        .unwrap_or_else(|| "Unknown".to_string());
    let principal = principal_override.or(user_ids);
    let target = target_override;
    let timestamp = timestamp_override.or(create_date);

    let raw_json = serde_json::to_string(raw).ok();

    CloudAuditEntry {
        source: CloudAuditSource::M365,
        action,
        principal,
        target,
        timestamp,
        raw: raw_json,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_data() {
        let result = parse_m365_audit_log("");
        assert!(result.is_err());
    }

    #[test]
    fn parse_m365_csv_with_audit_data() {
        let csv = r#"CreationDate,UserIds,Operations,AuditData
2024-06-15T12:00:00,alice@contoso.com,FileDownloaded,"{""Operation"":""FileDownloaded"",""UserId"":""alice@contoso.com"",""ObjectId"":""https://contoso.sharepoint.com/sites/team/Shared%20Documents/report.docx""}"
2024-06-15T12:05:00,bob@contoso.com,UserLoggedIn,"{""Operation"":""UserLoggedIn"",""UserId"":""bob@contoso.com"",""ClientIP"":""203.0.113.42""}"#;

        let entries = parse_m365_audit_log(csv).expect("should parse");
        assert_eq!(entries.len(), 2);

        let first = entries
            .iter()
            .find(|e| e.action == "FileDownloaded")
            .expect("FileDownloaded entry not found");
        assert_eq!(first.principal.as_deref(), Some("alice@contoso.com"));
        assert!(first.target.is_some());

        let second = entries
            .iter()
            .find(|e| e.action == "UserLoggedIn")
            .expect("UserLoggedIn entry not found");
        assert_eq!(second.principal.as_deref(), Some("bob@contoso.com"));
    }

    #[test]
    fn parse_m365_malformed_audit_data_not_fatal() {
        let csv = r#"CreationDate,UserIds,Operations,AuditData
2024-06-15T12:00:00,alice@contoso.com,FileAccessed,not-json"#;

        let entries = parse_m365_audit_log(csv).expect("should parse");
        assert_eq!(entries.len(), 1);
        // Malformed AuditData is not fatal; the Operation from the top-level column wins
        assert_eq!(entries[0].action, "FileAccessed");
    }
}
