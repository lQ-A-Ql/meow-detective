//! AWS CloudTrail JSON parser.
//!
//! Parses AWS CloudTrail log records (JSON format) and normalizes them into
//! `CloudAuditEntry` structs.
//!
//! CloudTrail record key fields:
//! - `eventVersion` — schema version
//! - `userIdentity` — principal details (type, arn, userName, etc.)
//! - `eventTime` — ISO 8601 timestamp
//! - `eventSource` — AWS service (e.g., "s3.amazonaws.com")
//! - `eventName` — API action (e.g., "PutObject")
//! - `awsRegion` — region
//! - `sourceIPAddress` — caller IP
//! - `requestParameters` — request details
//! - `responseElements` — response details
//! - `resources` — affected resources
//!
//! The file format is one JSON object per line (JSON Lines) or a
//! `{"Records": [...]}` wrapper.

use super::normalize::{CloudAuditEntry, CloudAuditSource};
use serde::{Deserialize, Serialize};

/// Raw parsed AWS CloudTrail record (subset of fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AwsCloudTrailRecord {
    #[serde(default)]
    pub event_version: Option<String>,
    #[serde(default)]
    pub user_identity: Option<serde_json::Value>,
    #[serde(default)]
    pub event_time: Option<String>,
    #[serde(default)]
    pub event_source: Option<String>,
    #[serde(default)]
    pub event_name: Option<String>,
    #[serde(default)]
    pub aws_region: Option<String>,
    #[serde(default)]
    pub source_ip_address: Option<String>,
    #[serde(default)]
    pub resources: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub request_parameters: Option<serde_json::Value>,
}

/// Parse an AWS CloudTrail log file (JSON Lines or {"Records":[...]} wrapper).
///
/// Returns a list of normalized `CloudAuditEntry` records.
pub fn parse_cloudtrail(data: &str) -> Result<Vec<CloudAuditEntry>, String> {
    if data.trim().is_empty() {
        return Err("CloudTrail data is empty".to_string());
    }

    let mut entries: Vec<CloudAuditEntry> = Vec::new();

    // Try to parse as a Records wrapper first
    if let Ok(wrapper) = serde_json::from_str::<serde_json::Value>(data.trim()) {
        if let Some(records) = wrapper.get("Records").and_then(|r| r.as_array()) {
            for record in records {
                if let Ok(ct) = serde_json::from_value::<AwsCloudTrailRecord>(record.clone()) {
                    entries.push(normalize_cloudtrail(&ct, record));
                }
            }
            return Ok(entries);
        }
    }

    // Fall back to JSON Lines (one record per line)
    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let value: serde_json::Value = serde_json::from_str(line)
            .map_err(|e| format!("Invalid CloudTrail JSON line: {}", e))?;
        if let Ok(ct) = serde_json::from_value::<AwsCloudTrailRecord>(value.clone()) {
            entries.push(normalize_cloudtrail(&ct, &value));
        }
    }

    if entries.is_empty() {
        return Err("No CloudTrail records found in data".to_string());
    }

    Ok(entries)
}

fn normalize_cloudtrail(record: &AwsCloudTrailRecord, raw: &serde_json::Value) -> CloudAuditEntry {
    let action = record
        .event_source
        .as_deref()
        .map(|src| {
            let svc = src
                .strip_suffix(".amazonaws.com")
                .unwrap_or(src)
                .split('.')
                .next()
                .unwrap_or(src);
            format!(
                "{}:{}",
                svc,
                record.event_name.as_deref().unwrap_or("Unknown")
            )
        })
        .unwrap_or_else(|| {
            record
                .event_name
                .clone()
                .unwrap_or_else(|| "Unknown".to_string())
        });

    let principal = record.user_identity.as_ref().and_then(|ui| {
        ui.get("arn")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                ui.get("userName")
                    .and_then(|v| v.as_str())
                    .map(|s| format!("user/{}", s))
            })
    });

    let target = record.resources.as_ref().and_then(|resources| {
        resources
            .first()
            .and_then(|r| r.get("ARN").or_else(|| r.get("arn")))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    });

    let raw_json = serde_json::to_string(raw).ok();

    CloudAuditEntry {
        source: CloudAuditSource::Aws,
        action,
        principal,
        target,
        timestamp: record.event_time.clone(),
        raw: raw_json,
    }
}

#[cfg(test)]
#[path = "../tests/unit/aws.rs"]
mod tests;
