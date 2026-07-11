use super::common::{insert_opt, truncate};
use crate::analysis_service::artifact_builders::{base_attrs, make_artifact, make_timeline_event};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::extraction::ExtractionOutcome;
use serde_json::Value;

pub(in crate::analysis_service::extraction) fn is_auth_log_path(normalized: &str) -> bool {
    normalized.ends_with("/var/log/auth.log")
        || normalized.contains("/var/log/auth.log.")
        || normalized.ends_with("/var/log/secure")
        || normalized.contains("/var/log/secure.")
}

pub(super) fn extract(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_auth_log_sudo(&text) {
        Ok(events) => {
            for event in events {
                let mut attrs = base_attrs(candidate);
                attrs.insert("user".to_string(), Value::String(event.user.clone()));
                attrs.insert("command".to_string(), Value::String(event.command.clone()));
                insert_opt(&mut attrs, "targetUser", event.target_user.clone());
                insert_opt(
                    &mut attrs,
                    "workingDirectory",
                    event.working_directory.clone(),
                );
                insert_opt(&mut attrs, "terminal", event.terminal.clone());
                attrs.insert("success".to_string(), Value::Bool(event.success));
                if let Some(timestamp) = event.timestamp {
                    attrs.insert(
                        "timestamp".to_string(),
                        Value::String(timestamp.to_rfc3339()),
                    );
                }

                outcome.artifacts.push(make_artifact(
                    "LinuxSudoEvent",
                    format!(
                        "sudo {} {} (success={})",
                        event.user,
                        truncate(&event.command, 60),
                        event.success
                    ),
                    format!(
                        "User {} invoked sudo as {:?}: {} (success={})",
                        event.user, event.target_user, event.command, event.success
                    ),
                    candidate,
                    "linux.sudo",
                    attrs.clone(),
                ));

                if let Some(timestamp) = event.timestamp {
                    outcome.timeline_events.push(make_timeline_event(
                        &candidate.file_id,
                        "linux.sudo",
                        timestamp,
                        format!("sudo {}: {}", event.user, truncate(&event.command, 80)),
                        format!(
                            "target_user={:?}, success={}",
                            event.target_user, event.success
                        ),
                        attrs,
                        "linux.sudo",
                    ));
                }
            }
        }
        Err(error) => outcome.warnings.push(format!(
            "{} sudo log parse failed: {}",
            candidate.path, error
        )),
    }
}
