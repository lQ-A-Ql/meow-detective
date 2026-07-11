use super::common::{insert_opt, truncate};
use crate::analysis_service::artifact_builders::{base_attrs, make_artifact, make_timeline_event};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::extraction::ExtractionOutcome;
use serde_json::Value;

pub(in crate::analysis_service::extraction) fn is_journal_path(normalized: &str) -> bool {
    normalized.ends_with(".journal")
        || normalized.ends_with(".journal~")
        || normalized.contains("/var/log/journal/")
        || normalized.contains("/run/log/journal/")
}

pub(super) fn extract(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    match artifacts_linux::parse_journal(bytes) {
        Ok(entries) => {
            for entry in entries {
                let mut attrs = base_attrs(candidate);
                insert_opt(&mut attrs, "message", entry.message.clone());
                insert_opt(&mut attrs, "executable", entry.executable.clone());
                insert_opt(&mut attrs, "cmdline", entry.cmdline.clone());
                insert_opt(&mut attrs, "systemdUnit", entry.systemd_unit.clone());
                insert_opt(&mut attrs, "hostname", entry.hostname.clone());
                insert_opt(
                    &mut attrs,
                    "syslogIdentifier",
                    entry.syslog_identifier.clone(),
                );
                insert_opt(&mut attrs, "bootId", entry.boot_id.clone());
                insert_opt(&mut attrs, "messageId", entry.message_id.clone());
                if let Some(pid) = entry.pid {
                    attrs.insert("pid".to_string(), Value::Number(pid.into()));
                }
                if let Some(uid) = entry.uid {
                    attrs.insert("uid".to_string(), Value::Number(uid.into()));
                }
                if let Some(priority) = entry.priority {
                    attrs.insert("priority".to_string(), Value::Number(priority.into()));
                }
                if !entry.raw_fields.is_empty() {
                    attrs.insert(
                        "rawFields".to_string(),
                        Value::Object(
                            entry
                                .raw_fields
                                .into_iter()
                                .map(|(key, value)| (key, Value::String(value)))
                                .collect(),
                        ),
                    );
                }
                if let Some(timestamp) = entry.timestamp {
                    attrs.insert(
                        "timestamp".to_string(),
                        Value::String(timestamp.to_rfc3339()),
                    );
                }

                let title = entry
                    .message
                    .as_deref()
                    .map(|message| format!("Journal: {}", truncate(message, 80)))
                    .unwrap_or_else(|| "Journal entry".to_string());
                outcome.artifacts.push(make_artifact(
                    "LinuxJournal",
                    title.clone(),
                    title,
                    candidate,
                    "linux.journal",
                    attrs.clone(),
                ));

                if let Some(timestamp) = entry.timestamp {
                    outcome.timeline_events.push(make_timeline_event(
                        &candidate.file_id,
                        "linux.journal",
                        timestamp,
                        "Journal message".to_string(),
                        entry.message.unwrap_or_default(),
                        attrs,
                        "linux.journal",
                    ));
                }
            }
        }
        Err(error) => outcome.warnings.push(format!(
            "{} journal parse failed: {}",
            candidate.path, error
        )),
    }
}
