use crate::analysis_service::artifact_builders::{base_attrs, make_artifact};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::extraction::ExtractionOutcome;
use serde_json::Value;

pub(super) fn extract(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let records = match artifacts_linux::parse_git_bytes_checked(&candidate.path, bytes) {
        Ok(records) => records,
        Err(error) => {
            outcome
                .warnings
                .push(format!("{}: {error}", candidate.path));
            return;
        }
    };
    if records.is_empty() {
        outcome.warnings.push(format!(
            "{} Git metadata produced no structured records",
            candidate.path
        ));
        return;
    }
    for record in records {
        let mut attrs = base_attrs(candidate);
        attrs.insert("recordType".into(), Value::String(record.kind.clone()));
        for name in [
            "lineNumber",
            "byteOffset",
            "offsetScope",
            "commitHash",
            "authorTime",
            "committerTime",
            "timestamp",
        ] {
            if let Some(value) = record.fields.get(name) {
                attrs.insert(name.into(), Value::String(value.clone()));
            }
        }
        attrs.insert(
            "fields".into(),
            serde_json::to_value(record.fields).unwrap_or(Value::Null),
        );
        let family = if candidate.path.contains("gitlab") {
            "GitLabArtifact"
        } else {
            "GitRepository"
        };
        outcome.artifacts.push(make_artifact(
            family,
            format!("{} metadata", record.kind),
            candidate.path.clone(),
            candidate,
            "linux.git",
            attrs,
        ));
    }
}
