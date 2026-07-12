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
