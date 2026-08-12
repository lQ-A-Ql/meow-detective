use super::common::{insert_opt, truncate, MAX_TEXT_LOG_EVENTS_PER_SOURCE};
use crate::analysis_service::artifact_builders::{base_attrs, make_artifact};
use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};
use crate::analysis_service::extraction::ExtractionOutcome;
use serde_json::Value;

pub(in crate::analysis_service::extraction) fn is_system_config_path(normalized: &str) -> bool {
    normalized.ends_with("/etc/os-release")
        || normalized.ends_with("/usr/lib/os-release")
        || normalized.ends_with("/etc/passwd")
        || normalized.ends_with("/etc/shadow")
        || normalized.ends_with("/etc/gshadow")
        || normalized.ends_with("/etc/group")
        || normalized.ends_with("/etc/hostname")
        || normalized.ends_with("/etc/hosts")
        || normalized.ends_with("/etc/fstab")
        || normalized.ends_with("/etc/resolv.conf")
        || normalized.ends_with("/etc/machine-id")
        || normalized.ends_with("/etc/login.defs")
        || normalized.ends_with("/etc/anacrontab")
}

pub(in crate::analysis_service::extraction) fn is_sudoers_path(normalized: &str) -> bool {
    normalized.ends_with("/etc/sudoers") || normalized.contains("/etc/sudoers.d/")
}

pub(in crate::analysis_service::extraction) fn is_ssh_text_path(normalized: &str) -> bool {
    normalized.contains("/.ssh/authorized_keys")
        || normalized.contains("/.ssh/known_hosts")
        || normalized.ends_with("/etc/ssh/ssh_config")
        || normalized.ends_with("/etc/ssh/sshd_config")
        || normalized.contains("/etc/ssh/ssh_config.d/")
        || normalized.contains("/etc/ssh/sshd_config.d/")
}

pub(in crate::analysis_service::extraction) fn is_ssh_candidate_path(normalized: &str) -> bool {
    is_ssh_text_path(normalized) || normalized.contains("/etc/ssh/")
}

pub(in crate::analysis_service::extraction) fn is_systemd_unit_path(normalized: &str) -> bool {
    normalized.contains("/etc/systemd/system/")
        || normalized.contains("/lib/systemd/system/")
        || normalized.contains("/usr/lib/systemd/system/")
}

pub(in crate::analysis_service::extraction) fn is_init_script_path(normalized: &str) -> bool {
    normalized.contains("/etc/init.d/") || normalized.ends_with("/etc/rc.local")
}

pub(in crate::analysis_service::extraction) fn is_profile_script_path(normalized: &str) -> bool {
    normalized.contains("/etc/profile.d/")
}

pub(super) fn extract(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let normalized = normalize_evidence_path(&candidate.path);
    let text = String::from_utf8_lossy(bytes);
    if normalized.ends_with("/etc/os-release") || normalized.ends_with("/usr/lib/os-release") {
        extract_os_release(candidate, &text, outcome);
    } else if normalized.ends_with("/etc/passwd") {
        extract_passwd(candidate, &text, outcome);
    } else if normalized.ends_with("/etc/shadow") || normalized.ends_with("/etc/gshadow") {
        extract_shadow(candidate, &text, &normalized, outcome);
    } else {
        extract_key_value_or_lines(candidate, &text, "linux.system_config", outcome);
    }
}

pub(super) fn extract_text_config(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    parser: &str,
    config_kind: &str,
    outcome: &mut ExtractionOutcome,
) {
    if std::str::from_utf8(bytes).is_err() {
        outcome.warnings.push(format!(
            "{} contains non-UTF-8 bytes; invalid sequences were replaced before Linux config extraction",
            candidate.path
        ));
    }

    let text = String::from_utf8_lossy(bytes);
    let emitted = extract_key_value_or_lines_with_kind(
        candidate,
        &text,
        parser,
        config_kind,
        "Linux config",
        outcome,
    );
    if emitted == 0 {
        outcome.warnings.push(format!(
            "{} contained no auditable non-comment Linux config records",
            candidate.path
        ));
    }
}

fn extract_os_release(candidate: &EvidenceCandidate, text: &str, outcome: &mut ExtractionOutcome) {
    match artifacts_linux::parse_os_release(text) {
        Ok(info) => {
            let mut attrs = base_attrs(candidate);
            attrs.insert(
                "configKind".to_string(),
                Value::String("osRelease".to_string()),
            );
            insert_opt(&mut attrs, "prettyName", info.pretty_name.clone());
            insert_opt(&mut attrs, "osId", info.id.clone());
            insert_opt(&mut attrs, "versionId", info.version_id.clone());
            if !info.fields.is_empty() {
                attrs.insert(
                    "fields".to_string(),
                    Value::Object(
                        info.fields
                            .into_iter()
                            .map(|(key, value)| (key, Value::String(value)))
                            .collect(),
                    ),
                );
            }

            let title = info
                .pretty_name
                .as_deref()
                .map(|name| format!("Linux OS: {name}"))
                .unwrap_or_else(|| "Linux OS release".to_string());
            outcome.artifacts.push(make_artifact(
                "LinuxSystemConfig",
                title.clone(),
                title,
                candidate,
                "linux.os_release",
                attrs,
            ));
        }
        Err(error) => outcome.warnings.push(format!(
            "{} os-release parse failed: {}",
            candidate.path, error
        )),
    }
}

fn extract_passwd(candidate: &EvidenceCandidate, text: &str, outcome: &mut ExtractionOutcome) {
    match artifacts_linux::parse_passwd(text) {
        Ok(accounts) => {
            for account in accounts {
                let mut attrs = base_attrs(candidate);
                attrs.insert(
                    "configKind".to_string(),
                    Value::String("passwdAccount".to_string()),
                );
                attrs.insert(
                    "username".to_string(),
                    Value::String(account.username.clone()),
                );
                attrs.insert("uid".to_string(), Value::Number(account.uid.into()));
                attrs.insert("gid".to_string(), Value::Number(account.gid.into()));
                attrs.insert("gecos".to_string(), Value::String(account.gecos.clone()));
                attrs.insert("home".to_string(), Value::String(account.home.clone()));
                attrs.insert("shell".to_string(), Value::String(account.shell.clone()));
                attrs.insert("isUidZero".to_string(), Value::Bool(account.uid == 0));
                attrs.insert(
                    "hasInteractiveShell".to_string(),
                    Value::Bool(is_interactive_shell(&account.shell)),
                );

                outcome.artifacts.push(make_artifact(
                    "LinuxSystemConfig",
                    format!("Linux account: {}", account.username),
                    format!(
                        "uid={} gid={} home={} shell={}",
                        account.uid, account.gid, account.home, account.shell
                    ),
                    candidate,
                    "linux.passwd",
                    attrs,
                ));
            }
        }
        Err(error) => outcome
            .warnings
            .push(format!("{} passwd parse failed: {}", candidate.path, error)),
    }
}

/// Extract password-state (never the hash itself) from `/etc/shadow` and
/// `/etc/gshadow` via the shared `parse_shadow_accounts` parser.
fn extract_shadow(
    candidate: &EvidenceCandidate,
    text: &str,
    normalized: &str,
    outcome: &mut ExtractionOutcome,
) {
    let (config_kind, title_kind) = if normalized.ends_with("/etc/gshadow") {
        ("gshadowAccount", "Linux group")
    } else {
        ("shadowAccount", "Linux account")
    };
    let accounts = artifacts_linux::parse_shadow_accounts(text);
    if accounts.is_empty() {
        outcome.warnings.push(format!(
            "{} contained no parseable account records",
            candidate.path
        ));
    }
    for account in accounts {
        let mut attrs = base_attrs(candidate);
        attrs.insert(
            "configKind".to_string(),
            Value::String(config_kind.to_string()),
        );
        attrs.insert(
            "username".to_string(),
            Value::String(account.username.clone()),
        );
        attrs.insert("hasPassword".to_string(), Value::Bool(account.has_password));
        attrs.insert("locked".to_string(), Value::Bool(account.locked));

        outcome.artifacts.push(make_artifact(
            "LinuxSystemConfig",
            format!("{title_kind} password state: {}", account.username),
            format!(
                "has_password={} locked={}",
                account.has_password, account.locked
            ),
            candidate,
            "linux.shadow",
            attrs,
        ));
    }
}

fn extract_key_value_or_lines(
    candidate: &EvidenceCandidate,
    text: &str,
    parser: &str,
    outcome: &mut ExtractionOutcome,
) {
    extract_key_value_or_lines_with_kind(
        candidate,
        text,
        parser,
        "textConfig",
        "Linux config",
        outcome,
    );
}

fn extract_key_value_or_lines_with_kind(
    candidate: &EvidenceCandidate,
    text: &str,
    parser: &str,
    config_kind: &str,
    title_prefix: &str,
    outcome: &mut ExtractionOutcome,
) -> usize {
    let mut emitted = 0usize;
    for (line_number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if emitted >= MAX_TEXT_LOG_EVENTS_PER_SOURCE {
            outcome.warnings.push(format!(
                "{} system config emitted first {} records only",
                candidate.path, MAX_TEXT_LOG_EVENTS_PER_SOURCE
            ));
            break;
        }

        let mut attrs = base_attrs(candidate);
        attrs.insert(
            "configKind".to_string(),
            Value::String(config_kind.to_string()),
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
            format!("{title_prefix}: {}", truncate(trimmed, 80)),
            trimmed.to_string(),
            candidate,
            parser,
            attrs,
        ));
        emitted += 1;
    }
    emitted
}

fn parse_config_pair(line: &str) -> Option<(&str, &str)> {
    if let Some((key, value)) = line.split_once('=') {
        let key = key.trim();
        let value = value.trim();
        if !key.is_empty() && !value.is_empty() {
            return Some((key, value));
        }
    }
    if let Some((key, value)) = line.split_once(':') {
        let key = key.trim();
        let value = value.trim();
        if !key.is_empty() && !value.is_empty() {
            return Some((key, value));
        }
    }
    None
}

fn is_interactive_shell(shell: &str) -> bool {
    let lower = shell.to_ascii_lowercase();
    !(lower.ends_with("/false")
        || lower.ends_with("/nologin")
        || lower.ends_with("/sync")
        || lower == "false"
        || lower == "nologin")
}
