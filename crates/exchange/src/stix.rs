//! STIX 2.1 bundle export from case correlation and artifact data.
//!
//! Produces a JSON STIX 2.1 bundle containing indicators (from correlation
//! leads), observed-data objects (from artifacts, registry values, and email
//! messages), and relationship objects linking leads to their supporting evidence.

use serde_json::Value;
use std::collections::HashMap;

use transport::dto::analysis::{EmailMessageDto, RegistryValueDto};
use transport::dto::artifacts::ArtifactRowDto;
use transport::dto::correlation::{CorrelationConfidenceDto, CorrelationLeadDto};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum StixError {
    #[error("Failed to serialize STIX bundle: {0}")]
    Serialize(#[from] serde_json::Error),
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Generate a STIX 2.1 object identifier with the given type prefix.
/// Format: `<prefix>--<uuid-v4>`
fn stix_id(prefix: &str) -> String {
    let u = uuid::Uuid::new_v4();
    format!("{}--{}", prefix, u)
}

/// Return an ISO 8601 timestamp for the current UTC instant.
fn iso_now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Map the internal correlation confidence to a STIX confidence value (0-100).
fn stix_confidence(conf: &CorrelationConfidenceDto) -> u32 {
    match conf {
        CorrelationConfidenceDto::Direct => 100,
        CorrelationConfidenceDto::Strong => 75,
        CorrelationConfidenceDto::Weak => 50,
        CorrelationConfidenceDto::Heuristic => 25,
    }
}

// ---------------------------------------------------------------------------
// Object builders
// ---------------------------------------------------------------------------

/// Build a STIX 2.1 pattern string from a correlation lead.
///
/// Uses the lead title as a file-path indicator when the lead is associated
/// with a file; the pattern is rendered as `[file:path = '<title>']`.
fn build_indicator_pattern(lead: &CorrelationLeadDto) -> String {
    format!("[file:path = '{}']", lead.title)
}

/// Create a STIX 2.1 `indicator` object from a correlation lead.
pub fn indicator_from_lead(lead: &CorrelationLeadDto) -> Value {
    let now = iso_now();
    let id = stix_id("indicator");
    let pattern = build_indicator_pattern(lead);

    serde_json::json!({
        "type": "indicator",
        "spec_version": "2.1",
        "id": id,
        "created": now,
        "modified": now,
        "name": lead.title,
        "description": lead.summary,
        "pattern": pattern,
        "pattern_type": "stix",
        "valid_from": now,
        "labels": lead.families,
        "confidence": stix_confidence(&lead.confidence),
    })
}

/// Create a STIX 2.1 `observed-data` object from an artifact row.
///
/// Returns `None` when the artifact type is not a supported family.
pub fn observed_data_from_artifact(artifact: &ArtifactRowDto) -> Option<Value> {
    let now = iso_now();
    let id = stix_id("observed-data");

    match artifact.artifact_type.as_str() {
        "LNK" => {
            let path = artifact
                .attrs
                .get("target_path")
                .or_else(|| artifact.attrs.get("targetPath"))
                .and_then(|v| v.as_str())
                .unwrap_or(&artifact.summary);

            let file_id = stix_id("file");
            Some(serde_json::json!({
                "type": "observed-data",
                "spec_version": "2.1",
                "id": id,
                "created": now,
                "modified": now,
                "first_observed": now,
                "last_observed": now,
                "number_observed": 1,
                "object_refs": [file_id.clone()],
                "objects": {
                    file_id: {
                        "type": "file",
                        "name": path,
                        "x_artifact_id": artifact.id,
                        "x_artifact_type": "LNK",
                    }
                }
            }))
        }
        "BrowserDownload" => {
            let url = artifact
                .attrs
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or(&artifact.summary);

            let url_id = stix_id("url");
            Some(serde_json::json!({
                "type": "observed-data",
                "spec_version": "2.1",
                "id": id,
                "created": now,
                "modified": now,
                "first_observed": now,
                "last_observed": now,
                "number_observed": 1,
                "object_refs": [url_id.clone()],
                "objects": {
                    url_id: {
                        "type": "url",
                        "value": url,
                        "x_artifact_id": artifact.id,
                        "x_artifact_type": "BrowserDownload",
                    }
                }
            }))
        }
        _ => None,
    }
}

/// Create a STIX 2.1 `observed-data` object from a registry value record.
pub fn observed_data_from_registry(reg: &RegistryValueDto) -> Value {
    let now = iso_now();
    let id = stix_id("observed-data");
    let key_id = stix_id("windows-registry-key");

    serde_json::json!({
        "type": "observed-data",
        "spec_version": "2.1",
        "id": id,
        "created": now,
        "modified": now,
        "first_observed": now,
        "last_observed": now,
        "number_observed": 1,
        "object_refs": [key_id.clone()],
        "objects": {
            key_id: {
                "type": "windows-registry-key",
                "key": reg.key_path,
                "values": [{
                    "name": reg.value_name,
                    "data": reg.data,
                    "data_type": reg.value_type,
                }],
                "x_hive_path": reg.hive_path,
                "x_artifact_id": reg.artifact_id,
            }
        }
    })
}

/// Extract a bare SMTP address from a display-name address like
/// `"Alice <alice@example.com>"` or return the trimmed input.
fn extract_smtp_address(raw: &str) -> String {
    let raw = raw.trim();
    if let Some(start) = raw.find('<') {
        if let Some(end) = raw.find('>') {
            if end > start {
                return raw[start + 1..end].trim().to_string();
            }
        }
    }
    raw.to_string()
}

/// Create a STIX 2.1 `observed-data` object from an email message record.
pub fn observed_data_from_email(email: &EmailMessageDto) -> Value {
    let now = iso_now();
    let id = stix_id("observed-data");
    let msg_id = stix_id("email-message");

    let mut addr_objects: HashMap<String, Value> = HashMap::new();
    let mut addr_refs: HashMap<String, String> = HashMap::new();

    let mut register_address = |raw: &str| {
        let smtp = extract_smtp_address(raw);
        if smtp.is_empty() {
            return None;
        }
        let ref_id = addr_refs.get(&smtp).cloned().unwrap_or_else(|| {
            let new_id = stix_id("email-addr");
            addr_objects.insert(
                new_id.clone(),
                serde_json::json!({
                    "type": "email-addr",
                    "value": smtp,
                }),
            );
            addr_refs.insert(smtp.clone(), new_id.clone());
            new_id
        });
        Some(ref_id)
    };

    let from_ref = register_address(&email.from);
    let to_refs: Vec<String> = email
        .to
        .iter()
        .filter_map(|a| register_address(a))
        .collect();
    let cc_refs: Vec<String> = email
        .cc
        .iter()
        .filter_map(|a| register_address(a))
        .collect();
    let bcc_refs: Vec<String> = email
        .bcc
        .iter()
        .filter_map(|a| register_address(a))
        .collect();

    let mut object_refs = vec![msg_id.clone()];
    object_refs.extend(addr_objects.keys().cloned());

    let mut objects = serde_json::Map::new();
    objects.insert(
        msg_id.clone(),
        serde_json::json!({
            "type": "email-message",
            "is_multipart": email.body_html.is_some() || !email.attachments.is_empty(),
            "date": email.sent_at,
            "from_ref": from_ref,
            "to_refs": to_refs,
            "cc_refs": cc_refs,
            "bcc_refs": bcc_refs,
            "subject": email.subject,
            "x_message_id": email.message_id,
            "x_artifact_id": email.artifact_id,
        }),
    );
    for (ref_id, obj) in addr_objects {
        objects.insert(ref_id, obj);
    }

    serde_json::json!({
        "type": "observed-data",
        "spec_version": "2.1",
        "id": id,
        "created": now,
        "modified": now,
        "first_observed": now,
        "last_observed": now,
        "number_observed": 1,
        "object_refs": object_refs,
        "objects": objects,
    })
}

/// Create a STIX 2.1 `relationship` object linking two STIX objects.
pub fn relationship(
    source_ref: &str,
    target_ref: &str,
    relationship_type: &str,
    description: &str,
) -> Value {
    let now = iso_now();

    serde_json::json!({
        "type": "relationship",
        "spec_version": "2.1",
        "id": stix_id("relationship"),
        "created": now,
        "modified": now,
        "relationship_type": relationship_type,
        "source_ref": source_ref,
        "target_ref": target_ref,
        "description": description,
    })
}

// ---------------------------------------------------------------------------
// Public entry-point
// ---------------------------------------------------------------------------

/// Export a full STIX 2.1 bundle as a pretty-printed JSON string.
///
/// Takes a case identifier and four lists of case data:
/// - `leads`          — correlation leads (become STIX indicators)
/// - `artifacts`      — artifact rows (become STIX observed-data for LNK /
///   BrowserDownload)
/// - `registry_values`— registry values (become STIX observed-data for
///   windows-registry-key)
/// - `emails`         — email messages (become STIX observed-data for
///   email-message)
///
/// Relationships are automatically created between indicators (from leads)
/// and the observed-data objects they reference via `supporting_node_ids`.
pub fn export_stix_bundle(
    _case_id: &str,
    leads: &[CorrelationLeadDto],
    artifacts: &[ArtifactRowDto],
    registry_values: &[RegistryValueDto],
    emails: &[EmailMessageDto],
) -> Result<String, StixError> {
    let mut objects: Vec<Value> = Vec::new();
    // Maps source object ids (lead/artifact/registry/email ids) to their
    // generated STIX identifiers so relationships can reference them.
    let mut id_map: HashMap<String, String> = HashMap::new();

    // 1. Indicators from correlation leads
    for lead in leads {
        let indicator = indicator_from_lead(lead);
        if let Some(sid) = indicator["id"].as_str() {
            id_map.insert(format!("lead:{}", lead.id), sid.to_string());
        }
        objects.push(indicator);
    }

    // 2. Observed-data from artifacts
    for artifact in artifacts {
        if let Some(obs) = observed_data_from_artifact(artifact) {
            if let Some(sid) = obs["id"].as_str() {
                id_map.insert(format!("artifact:{}", artifact.id), sid.to_string());
            }
            objects.push(obs);
        }
    }

    // 3. Observed-data from registry values
    for reg in registry_values {
        let obs = observed_data_from_registry(reg);
        if let Some(sid) = obs["id"].as_str() {
            id_map.insert(format!("registry:{}", reg.artifact_id), sid.to_string());
        }
        objects.push(obs);
    }

    // 4. Observed-data from email messages
    for email in emails {
        let obs = observed_data_from_email(email);
        if let Some(sid) = obs["id"].as_str() {
            id_map.insert(format!("email:{}", email.artifact_id), sid.to_string());
        }
        objects.push(obs);
    }

    // 5. Relationships: indicator -> supporting observed-data objects
    for lead in leads {
        let lead_key = format!("lead:{}", lead.id);
        if let Some(lead_stix_id) = id_map.get(&lead_key) {
            for node_id in &lead.supporting_node_ids {
                // node_id can be prefixed like "artifact:1" or "timeline:1"
                if let Some(node_stix_id) = id_map.get(node_id) {
                    objects.push(relationship(
                        lead_stix_id,
                        node_stix_id,
                        "indicates",
                        &format!(
                            "Lead '{}' relates to supporting node {}",
                            lead.title, node_id
                        ),
                    ));
                }
            }
        }
    }

    let bundle_id = stix_id("bundle");
    let bundle = serde_json::json!({
        "type": "bundle",
        "id": bundle_id,
        "objects": objects,
        "spec_version": "2.1"
    });

    Ok(serde_json::to_string_pretty(&bundle)?)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "../tests/unit/stix.rs"]
mod tests;
