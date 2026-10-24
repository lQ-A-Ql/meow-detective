use super::*;

fn test_candidate(path: &str) -> EvidenceCandidate {
    EvidenceCandidate {
        file_id: domain::FileEntryId("file-docker-metadata".to_string()),
        data_source_id: "ds-test".to_string(),
        partition_index: None,
        path: path.to_string(),
        size: 4096,
        encrypted: false,
        content_identity: "test:docker-metadata".to_string(),
        companions: Vec::new(),
        modified_at: None,
        evidence_kind: "test".to_string(),
        parser: "test".to_string(),
        category: "LinuxArtifacts".to_string(),
    }
}

#[test]
fn docker_container_and_image_metadata_are_projected_as_structured_rows() {
    let container = test_candidate("var/lib/docker/containers/abc/config.v2.json");
    let mut outcome = ExtractionOutcome::default();
    extract(
        &container,
        br#"{"Name":"/web","Config":{"Image":"nginx:latest","Cmd":["nginx","-g","daemon off;"]},"State":{"Running":true,"Paused":false,"Restarting":false}}"#,
        &mut outcome,
    );
    assert_eq!(outcome.artifacts.len(), 1);
    let attrs = &outcome.artifacts[0].attrs;
    assert_eq!(
        attrs.get("configKind").and_then(Value::as_str),
        Some("dockerContainer")
    );
    assert_eq!(attrs.get("name").and_then(Value::as_str), Some("web"));
    assert_eq!(
        attrs.get("value").and_then(Value::as_str),
        Some("nginx:latest")
    );
    assert_eq!(attrs.get("state").and_then(Value::as_str), Some("running"));

    let images = test_candidate("var/lib/docker/image/btrfs/repositories.json");
    let mut outcome = ExtractionOutcome::default();
    extract(
        &images,
        br#"{"Repositories":{"nginx":{"nginx:latest":"sha256:abc"}}}"#,
        &mut outcome,
    );
    assert_eq!(outcome.artifacts.len(), 1);
    let attrs = &outcome.artifacts[0].attrs;
    assert_eq!(
        attrs.get("configKind").and_then(Value::as_str),
        Some("dockerImage")
    );
    assert_eq!(attrs.get("name").and_then(Value::as_str), Some("nginx"));
    assert_eq!(attrs.get("key").and_then(Value::as_str), Some("latest"));
    assert_eq!(
        attrs.get("line").and_then(Value::as_str),
        Some("nginx:latest")
    );
}

#[test]
fn docker_container_state_preserves_paused_restarting_and_unknown_metadata() {
    for (state, expected) in [
        (
            serde_json::json!({"Running":true,"Paused":true}),
            Some("paused"),
        ),
        (
            serde_json::json!({"Running":true,"Restarting":true}),
            Some("restarting"),
        ),
        (
            serde_json::json!({"Running":false,"Dead":true}),
            Some("dead"),
        ),
        (
            serde_json::json!({"Running":false,"StartedAt":"0001-01-01T00:00:00Z"}),
            Some("created"),
        ),
        (
            serde_json::json!({"Running":false,"StartedAt":"2026-10-24T00:00:00Z"}),
            Some("exited"),
        ),
        (serde_json::json!({}), None),
    ] {
        assert_eq!(container_state(&state).as_deref(), expected);
    }
}

#[test]
fn invalid_docker_json_produces_diagnostics_without_artifacts() {
    for path in [
        "var/lib/docker/containers/abc/config.v2.json",
        "var/lib/docker/image/overlay2/repositories.json",
    ] {
        let mut outcome = ExtractionOutcome::default();
        extract(&test_candidate(path), b"{", &mut outcome);
        assert!(outcome.artifacts.is_empty());
        assert_eq!(outcome.warnings.len(), 1);
    }
}
