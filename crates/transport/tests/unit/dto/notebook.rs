use super::*;

// ── Notebook entry ──────────────────────────────────────────────────

#[test]
fn notebook_entry_dto_serializes_camel_case() {
    let entry = NotebookEntryDto {
        id: "entry-1".to_string(),
        case_id: "case-1".to_string(),
        parent_id: None,
        author: "investigator".to_string(),
        entry_type: NotebookEntryTypeDto::Observation,
        title: "Suspicious file noted".to_string(),
        body_markdown: "## Details\n\nFound `cmd.exe` with an unexpected hash.".to_string(),
        tags: vec!["suspicious".to_string(), "cmd".to_string()],
        status: NotebookEntryStatusDto::Draft,
        created_at: "2026-06-14T12:00:00Z".to_string(),
        updated_at: "2026-06-14T12:30:00Z".to_string(),
    };

    let json = serde_json::to_value(entry).unwrap();

    assert_eq!(json["id"], "entry-1");
    assert_eq!(json["caseId"], "case-1");
    assert_eq!(json["author"], "investigator");
    assert_eq!(json["entryType"], "observation");
    assert_eq!(json["title"], "Suspicious file noted");
    assert_eq!(
        json["bodyMarkdown"],
        "## Details\n\nFound `cmd.exe` with an unexpected hash."
    );
    assert_eq!(json["tags"][0], "suspicious");
    assert_eq!(json["tags"][1], "cmd");
    assert_eq!(json["status"], "draft");
    assert_eq!(json["createdAt"], "2026-06-14T12:00:00Z");
    assert_eq!(json["updatedAt"], "2026-06-14T12:30:00Z");
    // Optional fields with None should be absent
    assert!(json.get("parentId").is_none());
    // Ensure snake_case keys are absent
    assert!(json.get("case_id").is_none());
    assert!(json.get("entry_type").is_none());
    assert!(json.get("body_markdown").is_none());
}

#[test]
fn notebook_entry_dto_serializes_with_parent_id() {
    let entry = NotebookEntryDto {
        id: "entry-2".to_string(),
        case_id: "case-1".to_string(),
        parent_id: Some("entry-1".to_string()),
        author: "investigator".to_string(),
        entry_type: NotebookEntryTypeDto::Hypothesis,
        title: "Malware hypothesis".to_string(),
        body_markdown: "Possibly a dropper.".to_string(),
        tags: vec![],
        status: NotebookEntryStatusDto::Draft,
        created_at: "2026-06-14T13:00:00Z".to_string(),
        updated_at: "2026-06-14T13:00:00Z".to_string(),
    };

    let json = serde_json::to_value(entry).unwrap();

    assert_eq!(json["parentId"], "entry-1");
    assert_eq!(json["entryType"], "hypothesis");
    // Empty tags vec should be absent
    assert!(json.get("tags").is_none());
}

#[test]
fn notebook_entry_dto_roundtrip() {
    let entry = NotebookEntryDto {
        id: "entry-1".to_string(),
        case_id: "case-1".to_string(),
        parent_id: Some("parent-1".to_string()),
        author: "investigator".to_string(),
        entry_type: NotebookEntryTypeDto::Finding,
        title: "Confirmed persistence".to_string(),
        body_markdown: "Run key found.".to_string(),
        tags: vec!["persistence".to_string(), "registry".to_string()],
        status: NotebookEntryStatusDto::Reviewed,
        created_at: "2026-06-14T12:00:00Z".to_string(),
        updated_at: "2026-06-14T14:00:00Z".to_string(),
    };

    let json = serde_json::to_value(&entry).unwrap();
    let roundtripped: NotebookEntryDto = serde_json::from_value(json).unwrap();

    assert_eq!(entry, roundtripped);
}

// ── Evidence citation ───────────────────────────────────────────────

#[test]
fn evidence_citation_dto_serializes_camel_case() {
    let citation = EvidenceCitationDto {
        id: "cite-1".to_string(),
        entry_id: "entry-1".to_string(),
        target_node_type: GraphNodeTypeDto::File,
        target_node_id: "node-file-1".to_string(),
        display_label: "cmd.exe hash mismatch".to_string(),
        snippet: Some("SHA256: abcd1234...".to_string()),
        cited_at: "2026-06-14T12:05:00Z".to_string(),
    };

    let json = serde_json::to_value(citation).unwrap();

    assert_eq!(json["id"], "cite-1");
    assert_eq!(json["entryId"], "entry-1");
    assert_eq!(json["targetNodeType"], "file");
    assert_eq!(json["targetNodeId"], "node-file-1");
    assert_eq!(json["displayLabel"], "cmd.exe hash mismatch");
    assert_eq!(json["snippet"], "SHA256: abcd1234...");
    assert_eq!(json["citedAt"], "2026-06-14T12:05:00Z");
    // Ensure snake_case keys are absent
    assert!(json.get("entry_id").is_none());
    assert!(json.get("target_node_type").is_none());
    assert!(json.get("target_node_id").is_none());
    assert!(json.get("display_label").is_none());
    assert!(json.get("cited_at").is_none());
}

#[test]
fn evidence_citation_dto_omits_none_snippet() {
    let citation = EvidenceCitationDto {
        id: "cite-2".to_string(),
        entry_id: "entry-1".to_string(),
        target_node_type: GraphNodeTypeDto::Artifact,
        target_node_id: "node-artifact-1".to_string(),
        display_label: "LNK artifact".to_string(),
        snippet: None,
        cited_at: "2026-06-14T12:10:00Z".to_string(),
    };

    let json = serde_json::to_value(citation).unwrap();

    assert!(json.get("snippet").is_none());
}

#[test]
fn evidence_citation_dto_roundtrip() {
    let citation = EvidenceCitationDto {
        id: "cite-1".to_string(),
        entry_id: "entry-1".to_string(),
        target_node_type: GraphNodeTypeDto::File,
        target_node_id: "node-1".to_string(),
        display_label: "label".to_string(),
        snippet: Some("snippet text".to_string()),
        cited_at: "2026-06-14T12:00:00Z".to_string(),
    };

    let json = serde_json::to_value(&citation).unwrap();
    let roundtripped: EvidenceCitationDto = serde_json::from_value(json).unwrap();

    assert_eq!(citation, roundtripped);
}

// ── Investigation step ──────────────────────────────────────────────

#[test]
fn investigation_step_dto_serializes_camel_case() {
    let step = InvestigationStepDto {
        id: "step-1".to_string(),
        case_id: "case-1".to_string(),
        step_kind: "search".to_string(),
        params_json: r#"{"query":"malware","caseId":"case-1"}"#.to_string(),
        timestamp: "2026-06-14T12:00:00Z".to_string(),
        duration_ms: 1523,
        case_state_hash: Some("abc123hash".to_string()),
        success: true,
        error_code: None,
    };

    let json = serde_json::to_value(step).unwrap();

    assert_eq!(json["id"], "step-1");
    assert_eq!(json["caseId"], "case-1");
    assert_eq!(json["stepKind"], "search");
    assert_eq!(
        json["paramsJson"],
        r#"{"query":"malware","caseId":"case-1"}"#
    );
    assert_eq!(json["timestamp"], "2026-06-14T12:00:00Z");
    assert_eq!(json["durationMs"], 1523);
    assert_eq!(json["caseStateHash"], "abc123hash");
    assert_eq!(json["success"], true);
    assert!(json.get("errorCode").is_none());
    // Ensure snake_case keys are absent
    assert!(json.get("case_id").is_none());
    assert!(json.get("step_kind").is_none());
    assert!(json.get("params_json").is_none());
    assert!(json.get("duration_ms").is_none());
    assert!(json.get("case_state_hash").is_none());
    assert!(json.get("error_code").is_none());
}

#[test]
fn investigation_step_dto_serializes_failed_step() {
    let step = InvestigationStepDto {
        id: "step-2".to_string(),
        case_id: "case-1".to_string(),
        step_kind: "artifact_extract".to_string(),
        params_json: r#"{"family":"LNK"}"#.to_string(),
        timestamp: "2026-06-14T12:01:00Z".to_string(),
        duration_ms: 0,
        case_state_hash: None,
        success: false,
        error_code: Some("E_PARSE_FAILED".to_string()),
    };

    let json = serde_json::to_value(step).unwrap();

    assert_eq!(json["success"], false);
    assert_eq!(json["errorCode"], "E_PARSE_FAILED");
    assert!(json.get("caseStateHash").is_none());
}

#[test]
fn investigation_step_dto_roundtrip() {
    let step = InvestigationStepDto {
        id: "step-1".to_string(),
        case_id: "case-1".to_string(),
        step_kind: "timeline_query".to_string(),
        params_json: r#"{"start":"2026-01-01","end":"2026-06-14"}"#.to_string(),
        timestamp: "2026-06-14T12:00:00Z".to_string(),
        duration_ms: 2500,
        case_state_hash: Some("def456".to_string()),
        success: true,
        error_code: None,
    };

    let json = serde_json::to_value(&step).unwrap();
    let roundtripped: InvestigationStepDto = serde_json::from_value(json).unwrap();

    assert_eq!(step, roundtripped);
}

// ── Deserialization from camelCase JSON ─────────────────────────────

#[test]
fn notebook_entry_dto_deserializes_from_camel_case() {
    let json = serde_json::json!({
        "id": "entry-1",
        "caseId": "case-1",
        "parentId": "parent-1",
        "author": "investigator",
        "entryType": "finding",
        "title": "Test",
        "bodyMarkdown": "Body",
        "tags": ["tag1"],
        "status": "final",
        "createdAt": "2026-06-14T12:00:00Z",
        "updatedAt": "2026-06-14T12:00:00Z"
    });

    let entry: NotebookEntryDto = serde_json::from_value(json).unwrap();

    assert_eq!(entry.id, "entry-1");
    assert_eq!(entry.case_id, "case-1");
    assert_eq!(entry.parent_id, Some("parent-1".to_string()));
    assert_eq!(entry.entry_type, NotebookEntryTypeDto::Finding);
    assert_eq!(entry.status, NotebookEntryStatusDto::Final);
}

#[test]
fn evidence_citation_dto_deserializes_from_camel_case() {
    let json = serde_json::json!({
        "id": "cite-1",
        "entryId": "entry-1",
        "targetNodeType": "file",
        "targetNodeId": "node-1",
        "displayLabel": "label",
        "citedAt": "2026-06-14T12:00:00Z"
    });

    let citation: EvidenceCitationDto = serde_json::from_value(json).unwrap();

    assert_eq!(citation.id, "cite-1");
    assert_eq!(citation.entry_id, "entry-1");
    assert_eq!(citation.target_node_type, GraphNodeTypeDto::File);
    assert_eq!(citation.target_node_id, "node-1");
    assert_eq!(citation.snippet, None);
}

#[test]
fn investigation_step_dto_deserializes_from_camel_case() {
    let json = serde_json::json!({
        "id": "step-1",
        "caseId": "case-1",
        "stepKind": "search",
        "paramsJson": "{}",
        "timestamp": "2026-06-14T12:00:00Z",
        "durationMs": 100,
        "success": true
    });

    let step: InvestigationStepDto = serde_json::from_value(json).unwrap();

    assert_eq!(step.id, "step-1");
    assert_eq!(step.case_id, "case-1");
    assert_eq!(step.step_kind, "search");
    assert_eq!(step.params_json, "{}");
    assert_eq!(step.duration_ms, 100);
    assert_eq!(step.case_state_hash, None);
    assert!(step.success);
    assert_eq!(step.error_code, None);
}
