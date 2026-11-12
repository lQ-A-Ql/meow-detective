use super::common::insert_opt;
use crate::analysis_service::artifact_builders::{base_attrs, make_artifact};
use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};
use crate::analysis_service::extraction::ExtractionOutcome;
use serde_json::Value;

pub(super) fn is_container_metadata_path(normalized: &str) -> bool {
    normalized.contains("/var/lib/docker/containers/") && normalized.ends_with("/config.v2.json")
}

pub(super) fn is_repository_metadata_path(normalized: &str) -> bool {
    normalized.contains("/var/lib/docker/image/") && normalized.ends_with("/repositories.json")
}

pub(super) fn extract(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let normalized = normalize_evidence_path(&candidate.path);
    if is_container_metadata_path(&normalized) {
        extract_container(candidate, bytes, outcome);
    } else if is_repository_metadata_path(&normalized) {
        extract_repositories(candidate, bytes, outcome);
    }
}

fn extract_container(candidate: &EvidenceCandidate, bytes: &[u8], outcome: &mut ExtractionOutcome) {
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        outcome.warnings.push(format!(
            "{} Docker container metadata is not valid JSON",
            candidate.path
        ));
        return;
    };
    let name = value["Name"]
        .as_str()
        .map(|name| name.trim_start_matches('/').to_string());
    let image = value["Config"]["Image"].as_str().map(str::to_string);
    let state = container_state(&value["State"]);
    let command = value["Config"]["Cmd"].as_array().map(|items| {
        items
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    });
    let mut attrs = base_attrs(candidate);
    attrs.insert(
        "configKind".to_string(),
        Value::String("dockerContainer".to_string()),
    );
    insert_opt(&mut attrs, "name", name.clone());
    insert_opt(&mut attrs, "state", state);
    insert_opt(&mut attrs, "key", Some("image".to_string()));
    insert_opt(&mut attrs, "value", image.clone());
    insert_opt(&mut attrs, "description", command);
    attrs.insert(
        "line".to_string(),
        Value::String(name.unwrap_or_else(|| candidate.path.clone())),
    );
    attrs.insert("lineNumber".to_string(), Value::Number(1.into()));
    outcome.artifacts.push(make_artifact(
        "LinuxSystemConfig",
        format!(
            "Docker container: {}",
            image.unwrap_or_else(|| "unknown image".to_string())
        ),
        candidate.path.clone(),
        candidate,
        "linux.docker_container",
        attrs,
    ));
}

fn container_state(state: &Value) -> Option<String> {
    if let Some(status) = state["Status"].as_str() {
        return Some(status.to_string());
    }
    let status = if state["Running"].as_bool()? {
        if state["Paused"].as_bool() == Some(true) {
            "paused"
        } else if state["Restarting"].as_bool() == Some(true) {
            "restarting"
        } else {
            "running"
        }
    } else if state["RemovalInProgress"].as_bool() == Some(true) {
        "removing"
    } else if state["Dead"].as_bool() == Some(true) {
        "dead"
    } else {
        let started = chrono::DateTime::parse_from_rfc3339(state["StartedAt"].as_str()?).ok()?;
        if started == chrono::DateTime::parse_from_rfc3339("0001-01-01T00:00:00Z").ok()? {
            "created"
        } else {
            "exited"
        }
    };
    Some(status.to_string())
}

fn extract_repositories(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        outcome.warnings.push(format!(
            "{} Docker repository metadata is not valid JSON",
            candidate.path
        ));
        return;
    };
    let Some(root) = value.as_object() else {
        return;
    };
    let repositories = root
        .get("Repositories")
        .and_then(Value::as_object)
        .unwrap_or(root);
    for (repository, tags) in repositories {
        let Some(tags) = tags.as_object() else {
            continue;
        };
        for (tag, digest) in tags {
            let Some(digest) = digest.as_str() else {
                continue;
            };
            let mut attrs = base_attrs(candidate);
            let label = tag
                .strip_prefix(repository)
                .and_then(|suffix| suffix.strip_prefix(':'))
                .unwrap_or(tag);
            let reference = if tag.starts_with(&format!("{repository}:"))
                || tag.starts_with(&format!("{repository}@"))
            {
                tag.clone()
            } else {
                format!("{repository}:{tag}")
            };
            attrs.insert(
                "configKind".to_string(),
                Value::String("dockerImage".to_string()),
            );
            attrs.insert("name".to_string(), Value::String(repository.clone()));
            attrs.insert("key".to_string(), Value::String(label.to_string()));
            attrs.insert("value".to_string(), Value::String(digest.to_string()));
            attrs.insert("line".to_string(), Value::String(reference.clone()));
            attrs.insert("lineNumber".to_string(), Value::Number(1.into()));
            outcome.artifacts.push(make_artifact(
                "LinuxSystemConfig",
                format!("Docker image: {reference}"),
                candidate.path.clone(),
                candidate,
                "linux.docker_image",
                attrs,
            ));
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/analysis_service/extraction/linux/docker.rs"]
mod tests;
