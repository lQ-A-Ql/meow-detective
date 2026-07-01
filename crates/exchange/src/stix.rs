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
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    // ------------------------------------------------------------------
    // Helpers for constructing DTOs in tests
    // ------------------------------------------------------------------

    fn make_lead(
        id: &str,
        title: &str,
        families: Vec<&str>,
        node_ids: Vec<&str>,
    ) -> CorrelationLeadDto {
        CorrelationLeadDto {
            id: id.to_string(),
            title: title.to_string(),
            summary: format!("Summary for {title}"),
            confidence: CorrelationConfidenceDto::Direct,
            families: families.into_iter().map(String::from).collect(),
            primary_file_id: "file-1".to_string(),
            supporting_node_ids: node_ids.into_iter().map(String::from).collect(),
            match_signals: vec![],
            jumps: vec![],
            provenance: vec![],
            caveats: vec![],
        }
    }

    fn make_artifact(
        id: &str,
        artifact_type: &str,
        summary: &str,
        attrs: BTreeMap<&str, &str>,
    ) -> ArtifactRowDto {
        let attrs: BTreeMap<String, Value> = attrs
            .into_iter()
            .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
            .collect();
        ArtifactRowDto {
            id: id.to_string(),
            artifact_type: artifact_type.to_string(),
            title: format!("{artifact_type} artifact"),
            summary: summary.to_string(),
            source_object_id: None,
            extractor_id: None,
            extractor_version: None,
            confidence: None,
            source_attribution: None,
            created_at: "2026-06-17T00:00:00Z".to_string(),
            attrs,
        }
    }

    fn make_registry(artifact_id: &str, key_path: &str) -> RegistryValueDto {
        RegistryValueDto {
            artifact_id: artifact_id.to_string(),
            file_id: "file-1".to_string(),
            source_path: "C:\\Windows\\System32\\config\\SOFTWARE".to_string(),
            hive_path: "SOFTWARE".to_string(),
            key_path: key_path.to_string(),
            value_name: "TestValue".to_string(),
            value_type: "REG_SZ".to_string(),
            data: "test_data".to_string(),
            parser: "reg".to_string(),
            created_at: "2026-06-17T00:00:00Z".to_string(),
        }
    }

    fn make_email(artifact_id: &str, subject: &str) -> EmailMessageDto {
        EmailMessageDto {
            artifact_id: artifact_id.to_string(),
            file_id: "file-1".to_string(),
            source_path: "mailbox.pst".to_string(),
            sent_at: Some("2026-06-17T00:00:00Z".to_string()),
            received_at: None,
            from: "attacker@evil.com".to_string(),
            to: vec!["victim@corp.com".to_string()],
            cc: vec![],
            bcc: vec![],
            reply_to: None,
            return_path: None,
            subject: subject.to_string(),
            message_id: "<msg-1@evil.com>".to_string(),
            in_reply_to: None,
            references: vec![],
            attachments: vec!["payload.exe".to_string()],
            attachment_details: vec![],
            headers: vec![],
            body_preview: "Click the link...".to_string(),
            body_plain: Some("Click the link...".to_string()),
            body_html: None,
            x_mailer: None,
            x_originating_ip: None,
            container_path: None,
            message_class: None,
            attachment_count: 1,
            is_deleted: Some(false),
        }
    }

    // ------------------------------------------------------------------
    // The required tests
    // ------------------------------------------------------------------

    #[test]
    fn test_export_stix_bundle_produces_valid_json() {
        let leads = vec![make_lead(
            "lead-1",
            "cmd.exe",
            vec!["LNK"],
            vec!["artifact:artifact-1"],
        )];
        let artifacts = vec![make_artifact(
            "artifact-1",
            "LNK",
            "C:\\Users\\victim\\malicious.exe",
            BTreeMap::from([("target_path", "C:\\Users\\victim\\malicious.exe")]),
        )];

        let result = export_stix_bundle("case-1", &leads, &artifacts, &[], &[]).unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["type"], "bundle");
        assert_eq!(parsed["spec_version"], "2.1");
        assert!(parsed["id"].as_str().unwrap().starts_with("bundle--"));
        assert!(parsed["objects"].is_array());
        let objs = parsed["objects"].as_array().unwrap();
        assert!(!objs.is_empty());

        // Verify the objects include at least one indicator and one observed-data
        let has_indicator = objs.iter().any(|o| o["type"] == "indicator");
        let has_observed = objs.iter().any(|o| o["type"] == "observed-data");
        assert!(has_indicator);
        assert!(has_observed);
    }

    #[test]
    fn test_indicator_from_correlation_lead() {
        let lead = make_lead("lead-1", "suspicious.exe", vec!["Prefetch"], vec![]);

        let indicator = indicator_from_lead(&lead);

        assert_eq!(indicator["type"], "indicator");
        assert_eq!(indicator["spec_version"], "2.1");
        assert!(indicator["id"].as_str().unwrap().starts_with("indicator--"));
        assert_eq!(indicator["name"], "suspicious.exe");
        assert_eq!(indicator["confidence"], 100); // Direct -> 100
        assert!(indicator["pattern"]
            .as_str()
            .unwrap()
            .contains("suspicious.exe"));
        let labels = indicator["labels"].as_array().unwrap();
        assert!(labels.contains(&serde_json::json!("Prefetch")));
        // created and modified should be identical ISO 8601 strings
        assert_eq!(indicator["created"], indicator["modified"]);
        assert!(!indicator["created"].as_str().unwrap().is_empty());
    }

    #[test]
    fn test_empty_case_produces_empty_bundle() {
        let result = export_stix_bundle("case-1", &[], &[], &[], &[]).unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["type"], "bundle");
        assert_eq!(parsed["spec_version"], "2.1");
        let objs = parsed["objects"].as_array().unwrap();
        assert!(objs.is_empty());
    }

    #[test]
    fn test_observed_data_from_artifact() {
        let artifact = make_artifact(
            "artifact-lnk",
            "LNK",
            "C:\\test.exe",
            BTreeMap::from([("target_path", "C:\\test.exe")]),
        );

        let obs = observed_data_from_artifact(&artifact).unwrap();

        assert_eq!(obs["type"], "observed-data");
        assert!(obs["id"].as_str().unwrap().starts_with("observed-data--"));
        assert_eq!(obs["number_observed"], 1);

        // The inner SCO is keyed by its STIX id inside `objects`.
        let refs = obs["object_refs"].as_array().unwrap();
        assert_eq!(refs.len(), 1);
        let file_id = refs[0].as_str().unwrap();
        let inner = &obs["objects"][file_id];
        assert_eq!(inner["type"], "file");
        assert_eq!(inner["name"], "C:\\test.exe");
        assert_eq!(inner["x_artifact_type"], "LNK");
    }

    // ------------------------------------------------------------------
    // Additional coverage tests
    // ------------------------------------------------------------------

    #[test]
    fn test_observed_data_from_browser_download_artifact() {
        let mut attrs: BTreeMap<String, Value> = BTreeMap::new();
        attrs.insert(
            "url".to_string(),
            Value::String("https://evil.com/payload.exe".to_string()),
        );

        let artifact = ArtifactRowDto {
            id: "artifact-bd".to_string(),
            artifact_type: "BrowserDownload".to_string(),
            title: "browser download".to_string(),
            summary: "downloaded payload".to_string(),
            source_object_id: None,
            extractor_id: None,
            extractor_version: None,
            confidence: None,
            source_attribution: None,
            created_at: "2026-06-17T00:00:00Z".to_string(),
            attrs,
        };

        let obs = observed_data_from_artifact(&artifact).unwrap();
        assert_eq!(obs["type"], "observed-data");

        let refs = obs["object_refs"].as_array().unwrap();
        let url_id = refs[0].as_str().unwrap();
        let inner = &obs["objects"][url_id];
        assert_eq!(inner["type"], "url");
        assert_eq!(inner["value"], "https://evil.com/payload.exe");
        assert_eq!(inner["x_artifact_type"], "BrowserDownload");
    }

    #[test]
    fn test_observed_data_from_registry() {
        let reg = make_registry(
            "artifact-reg",
            "HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
        );

        let obs = observed_data_from_registry(&reg);

        assert_eq!(obs["type"], "observed-data");
        let refs = obs["object_refs"].as_array().unwrap();
        let key_id = refs[0].as_str().unwrap();
        let inner = &obs["objects"][key_id];
        assert_eq!(inner["type"], "windows-registry-key");
        assert_eq!(
            inner["key"],
            "HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Run"
        );
        assert_eq!(inner["x_hive_path"], "SOFTWARE");
        assert_eq!(inner["values"][0]["name"], "TestValue");
    }

    #[test]
    fn test_observed_data_from_email() {
        let email = make_email("artifact-email", "Urgent: Password reset");

        let obs = observed_data_from_email(&email);

        assert_eq!(obs["type"], "observed-data");
        let refs = obs["object_refs"].as_array().unwrap();
        assert_eq!(refs.len(), 3); // email-message + from + to
        let msg_id = refs[0].as_str().unwrap();
        let inner = &obs["objects"][msg_id];
        assert_eq!(inner["type"], "email-message");
        assert_eq!(inner["subject"], "Urgent: Password reset");

        let from_id = inner["from_ref"].as_str().unwrap();
        assert_eq!(obs["objects"][from_id]["type"], "email-addr");
        assert_eq!(obs["objects"][from_id]["value"], "attacker@evil.com");

        let to_id = inner["to_refs"][0].as_str().unwrap();
        assert_eq!(obs["objects"][to_id]["type"], "email-addr");
        assert_eq!(obs["objects"][to_id]["value"], "victim@corp.com");

        assert_eq!(inner["x_message_id"], "<msg-1@evil.com>");
    }

    #[test]
    fn test_stix_confidence_mapping() {
        assert_eq!(stix_confidence(&CorrelationConfidenceDto::Direct), 100);
        assert_eq!(stix_confidence(&CorrelationConfidenceDto::Strong), 75);
        assert_eq!(stix_confidence(&CorrelationConfidenceDto::Weak), 50);
        assert_eq!(stix_confidence(&CorrelationConfidenceDto::Heuristic), 25);
    }

    #[test]
    fn test_relationship_object() {
        let rel = relationship(
            "indicator--abc",
            "observed-data--def",
            "indicates",
            "lead relates to artifact",
        );

        assert_eq!(rel["type"], "relationship");
        assert!(rel["id"].as_str().unwrap().starts_with("relationship--"));
        assert_eq!(rel["relationship_type"], "indicates");
        assert_eq!(rel["source_ref"], "indicator--abc");
        assert_eq!(rel["target_ref"], "observed-data--def");
        assert_eq!(rel["description"], "lead relates to artifact");
    }

    #[test]
    fn test_unsupported_artifact_type_returns_none() {
        let artifact = make_artifact(
            "artifact-unknown",
            "UnknownType",
            "unknown",
            BTreeMap::new(),
        );
        assert!(observed_data_from_artifact(&artifact).is_none());
    }

    #[test]
    fn test_lnk_artifact_falls_back_to_summary_when_target_path_missing() {
        let artifact = make_artifact(
            "artifact-lnk",
            "LNK",
            "C:\\Program Files\\app.exe",
            BTreeMap::new(),
        );

        let obs = observed_data_from_artifact(&artifact).unwrap();
        let refs = obs["object_refs"].as_array().unwrap();
        let file_id = refs[0].as_str().unwrap();
        assert_eq!(
            obs["objects"][file_id]["name"],
            "C:\\Program Files\\app.exe"
        );
    }

    #[test]
    fn test_browser_download_falls_back_to_summary_when_url_missing() {
        let artifact = make_artifact(
            "artifact-bd",
            "BrowserDownload",
            "https://fallback.example.com/bad.exe",
            BTreeMap::new(),
        );

        let obs = observed_data_from_artifact(&artifact).unwrap();
        let refs = obs["object_refs"].as_array().unwrap();
        let url_id = refs[0].as_str().unwrap();
        assert_eq!(
            obs["objects"][url_id]["value"],
            "https://fallback.example.com/bad.exe"
        );
    }

    #[test]
    fn test_relationships_generated_for_lead_supporting_nodes() {
        let leads = vec![make_lead(
            "lead-1",
            "malware.exe",
            vec!["LNK"],
            vec!["artifact:artifact-lnk"],
        )];
        let artifacts = vec![make_artifact(
            "artifact-lnk",
            "LNK",
            "C:\\malware.exe",
            BTreeMap::from([("target_path", "C:\\malware.exe")]),
        )];

        let result = export_stix_bundle("case-1", &leads, &artifacts, &[], &[]).unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        let objs = parsed["objects"].as_array().unwrap();

        let rel_count = objs.iter().filter(|o| o["type"] == "relationship").count();
        assert!(rel_count >= 1, "Expected at least one relationship object");

        let rel = objs.iter().find(|o| o["type"] == "relationship").unwrap();
        assert_eq!(rel["relationship_type"], "indicates");
        assert!(rel["source_ref"]
            .as_str()
            .unwrap()
            .starts_with("indicator--"));
        assert!(rel["target_ref"]
            .as_str()
            .unwrap()
            .starts_with("observed-data--"));
    }
}
