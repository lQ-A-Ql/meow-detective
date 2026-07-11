use crate::analysis_service::artifact_builders::{base_attrs, make_artifact, make_timeline_event};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::extraction::ExtractionOutcome;
use serde_json::Value;

pub(in crate::analysis_service::extraction) fn is_apt_history_path(normalized: &str) -> bool {
    normalized.contains("/var/log/apt/history.log")
}

pub(in crate::analysis_service::extraction) fn is_dpkg_log_path(normalized: &str) -> bool {
    normalized.contains("/var/log/dpkg.log")
}

pub(in crate::analysis_service::extraction) fn is_rpm_package_log_path(normalized: &str) -> bool {
    normalized.ends_with("/var/log/yum.log")
        || normalized.contains("/var/log/yum.log.")
        || normalized.ends_with("/var/log/dnf.log")
        || normalized.contains("/var/log/dnf.log.")
        || normalized.ends_with("/var/log/dnf.rpm.log")
        || normalized.contains("/var/log/dnf.rpm.log.")
}

pub(super) fn extract_apt_history(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_apt_history(&text) {
        Ok(events) => {
            for event in events {
                push_event(candidate, event, outcome);
            }
        }
        Err(error) => outcome.warnings.push(format!(
            "{} APT history parse failed: {}",
            candidate.path, error
        )),
    }
}

pub(super) fn extract_dpkg_log(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_dpkg_log(&text) {
        Ok(events) => {
            for event in events {
                push_event(candidate, event, outcome);
            }
        }
        Err(error) => outcome.warnings.push(format!(
            "{} dpkg log parse failed: {}",
            candidate.path, error
        )),
    }
}

pub(super) fn extract_rpm_package_log(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_rpm_package_log(&text) {
        Ok(events) => {
            for event in events {
                push_event(candidate, event, outcome);
            }
        }
        Err(error) => outcome.warnings.push(format!(
            "{} rpm package log parse failed: {}",
            candidate.path, error
        )),
    }
}

fn push_event(
    candidate: &EvidenceCandidate,
    event: artifacts_linux::AptEvent,
    outcome: &mut ExtractionOutcome,
) {
    let mut attrs = base_attrs(candidate);
    attrs.insert("action".to_string(), Value::String(event.action.clone()));
    attrs.insert("package".to_string(), Value::String(event.package.clone()));
    attrs.insert("version".to_string(), Value::String(event.version.clone()));
    if let Some(timestamp) = event.timestamp {
        attrs.insert(
            "timestamp".to_string(),
            Value::String(timestamp.to_rfc3339()),
        );
    }

    outcome.artifacts.push(make_artifact(
        "LinuxAptEvent",
        format!("APT/DPKG {} {}", event.action, event.package),
        format!(
            "{} {} {} ({})",
            event.action,
            event.package,
            event.version,
            event
                .timestamp
                .map(|timestamp| timestamp.to_rfc3339())
                .unwrap_or_default()
        ),
        candidate,
        "linux.apt",
        attrs.clone(),
    ));

    if let Some(timestamp) = event.timestamp {
        outcome.timeline_events.push(make_timeline_event(
            &candidate.file_id,
            "linux.apt",
            timestamp,
            format!(
                "Package {} {} {}",
                event.action, event.package, event.version
            ),
            format!("Logged by {} parser", candidate.path),
            attrs,
            "linux.apt",
        ));
    }
}
