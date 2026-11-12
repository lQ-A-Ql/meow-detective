use super::common::{truncate, MAX_TEXT_LOG_EVENTS_PER_SOURCE};
use crate::analysis_service::artifact_builders::{base_attrs, make_artifact};
use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};
use crate::analysis_service::extraction::ExtractionOutcome;
use serde_json::Value;

pub(in crate::analysis_service::extraction) fn is_pve_config_path(normalized: &str) -> bool {
    normalized.ends_with("/etc/pve/storage.cfg")
        || (normalized.contains("/etc/pve/qemu-server/") && normalized.ends_with(".conf"))
        || (normalized.contains("/etc/pve/lxc/") && normalized.ends_with(".conf"))
        || normalized.ends_with("/etc/pve/corosync.conf")
        || normalized.ends_with("/etc/corosync/corosync.conf")
}

pub(in crate::analysis_service::extraction) fn is_pve_log_path(normalized: &str) -> bool {
    normalized.ends_with("/var/log/pveproxy/access.log")
        || normalized.contains("/var/log/pveproxy/access.log.")
        || normalized.ends_with("/var/log/pvedaemon.log")
        || normalized.contains("/var/log/pvedaemon.log.")
        || normalized.contains("/var/log/pve/tasks/")
}

pub(super) fn extract_config(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    if std::str::from_utf8(bytes).is_err() {
        outcome.warnings.push(format!(
            "{} contains non-UTF-8 bytes; invalid sequences were replaced before PVE config extraction",
            candidate.path
        ));
    }

    let normalized = normalize_evidence_path(&candidate.path);
    let config_type = config_type(&normalized);
    let text = String::from_utf8_lossy(bytes);
    let mut emitted = 0usize;
    for (line_number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if emitted >= MAX_TEXT_LOG_EVENTS_PER_SOURCE {
            outcome.warnings.push(format!(
                "{} PVE config emitted first {} records only",
                candidate.path, MAX_TEXT_LOG_EVENTS_PER_SOURCE
            ));
            break;
        }

        let mut attrs = base_attrs(candidate);
        attrs.insert(
            "configKind".to_string(),
            Value::String("pveConfig".to_string()),
        );
        attrs.insert(
            "pveConfigType".to_string(),
            Value::String(config_type.to_string()),
        );
        attrs.insert("line".to_string(), Value::String(trimmed.to_string()));
        attrs.insert(
            "lineNumber".to_string(),
            Value::Number((line_number as u64 + 1).into()),
        );
        if let Some((key, value)) = parse_config_pair(trimmed) {
            attrs.insert("key".to_string(), Value::String(key.to_string()));
            attrs.insert("value".to_string(), Value::String(value.to_string()));
        }

        outcome.artifacts.push(make_artifact(
            "LinuxSystemConfig",
            format!("PVE config: {}", truncate(trimmed, 80)),
            trimmed.to_string(),
            candidate,
            "linux.pve_config",
            attrs,
        ));
        emitted += 1;
    }

    if emitted == 0 {
        outcome.warnings.push(format!(
            "{} PVE config contained no auditable non-comment records",
            candidate.path
        ));
    }
}

fn config_type(normalized: &str) -> &'static str {
    if normalized.ends_with("/etc/pve/storage.cfg") {
        "pveStorageConfig"
    } else if normalized.contains("/etc/pve/qemu-server/") {
        "pveQemuConfig"
    } else if normalized.contains("/etc/pve/lxc/") {
        "pveLxcConfig"
    } else if normalized.ends_with("/etc/pve/corosync.conf")
        || normalized.ends_with("/etc/corosync/corosync.conf")
    {
        "pveCorosyncConfig"
    } else {
        "pveConfig"
    }
}

fn parse_config_pair(line: &str) -> Option<(&str, &str)> {
    if let Some((key, value)) = line.split_once(':') {
        let key = key.trim();
        let value = value.trim();
        if !key.is_empty() && !value.is_empty() {
            return Some((key, value));
        }
    }
    if let Some((key, value)) = line.split_once('=') {
        let key = key.trim();
        let value = value.trim();
        if !key.is_empty() && !value.is_empty() {
            return Some((key, value));
        }
    }

    let mut parts = line.splitn(2, char::is_whitespace);
    let key = parts.next()?.trim();
    let value = parts.next()?.trim();
    if key.is_empty() || value.is_empty() || value == "{" || value == "}" {
        None
    } else {
        Some((key, value))
    }
}
