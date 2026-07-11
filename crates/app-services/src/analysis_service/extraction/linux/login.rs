use crate::analysis_service::artifact_builders::{base_attrs, make_artifact, make_timeline_event};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::extraction::ExtractionOutcome;
use serde_json::Value;

pub(in crate::analysis_service::extraction) fn is_wtmp_path(normalized: &str) -> bool {
    normalized.ends_with("/var/log/wtmp")
        || normalized.contains("/var/log/wtmp.")
        || normalized.ends_with("/var/log/btmp")
        || normalized.contains("/var/log/btmp.")
}

pub(in crate::analysis_service::extraction) fn is_login_binary_candidate_path(
    normalized: &str,
) -> bool {
    is_wtmp_path(normalized)
        || normalized.ends_with("/var/log/lastlog")
        || normalized.ends_with("/var/log/faillog")
}

pub(super) fn extract(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    match artifacts_linux::parse_wtmp(bytes) {
        Ok(records) => {
            for record in records {
                let mut attrs = base_attrs(candidate);
                attrs.insert("user".to_string(), Value::String(record.user.clone()));
                attrs.insert(
                    "terminal".to_string(),
                    Value::String(record.terminal.clone()),
                );
                attrs.insert("host".to_string(), Value::String(record.host.clone()));
                attrs.insert("pid".to_string(), Value::Number(record.pid.into()));
                attrs.insert(
                    "recordType".to_string(),
                    Value::Number(record.record_type.into()),
                );
                if let Some(timestamp) = record.login_time {
                    attrs.insert(
                        "loginTime".to_string(),
                        Value::String(timestamp.to_rfc3339()),
                    );
                }
                if let Some(timestamp) = record.logout_time {
                    attrs.insert(
                        "logoutTime".to_string(),
                        Value::String(timestamp.to_rfc3339()),
                    );
                }

                let (event_type, title) = event_title(&record);
                outcome.artifacts.push(make_artifact(
                    "LinuxWtmp",
                    title.clone(),
                    title,
                    candidate,
                    "linux.wtmp",
                    attrs.clone(),
                ));

                if let Some(timestamp) = record.login_time {
                    outcome.timeline_events.push(make_timeline_event(
                        &candidate.file_id,
                        event_type,
                        timestamp,
                        format!(
                            "{} {}@{} ({})",
                            event_type, record.user, record.host, record.terminal
                        ),
                        format!("PID {} record_type {}", record.pid, record.record_type),
                        attrs.clone(),
                        "linux.wtmp",
                    ));
                }
                if let Some(timestamp) = record.logout_time {
                    outcome.timeline_events.push(make_timeline_event(
                        &candidate.file_id,
                        "logout",
                        timestamp,
                        format!(
                            "Logout {}@{} ({})",
                            record.user, record.host, record.terminal
                        ),
                        format!("PID {} record_type {}", record.pid, record.record_type),
                        attrs.clone(),
                        "linux.wtmp",
                    ));
                }
            }
        }
        Err(error) => outcome
            .warnings
            .push(format!("{} wtmp parse failed: {}", candidate.path, error)),
    }
}

fn event_title(record: &artifacts_linux::LoginRecord) -> (&'static str, String) {
    match record.record_type {
        2 => (
            "boot",
            format!(
                "Boot: {}",
                record
                    .login_time
                    .map(|timestamp| timestamp.to_rfc3339())
                    .unwrap_or_default()
            ),
        ),
        1 => ("runlevel", format!("Runlevel: {}", record.user)),
        _ => (
            "login",
            format!(
                "Login {}@{} via {} ({})",
                record.user,
                record.host,
                record.terminal,
                record
                    .login_time
                    .map(|timestamp| timestamp.to_rfc3339())
                    .unwrap_or_default()
            ),
        ),
    }
}
