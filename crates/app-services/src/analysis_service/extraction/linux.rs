use super::ExtractionOutcome;
use crate::analysis_service::artifact_builders::{base_attrs, make_artifact, make_timeline_event};
use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};
use serde_json::Value;
use std::collections::BTreeMap;

pub fn extract_linux_candidate(candidate: &EvidenceCandidate, bytes: &[u8]) -> ExtractionOutcome {
    let mut outcome = ExtractionOutcome::default();
    let normalized = normalize_evidence_path(&candidate.path);

    if normalized.ends_with(".journal") || normalized.contains("/var/log/journal/") {
        extract_journal(candidate, bytes, &mut outcome);
    } else if normalized.ends_with("/var/log/wtmp") || normalized.ends_with("/var/log/btmp") {
        extract_wtmp(candidate, bytes, &mut outcome);
    } else if normalized.ends_with(".bash_history") {
        extract_bash_history(candidate, bytes, &mut outcome);
    } else if normalized.contains("/var/log/apt/history.log") {
        extract_apt_history(candidate, bytes, &mut outcome);
    } else if normalized.contains("/var/log/dpkg.log") {
        extract_dpkg_log(candidate, bytes, &mut outcome);
    } else if normalized.ends_with("/etc/crontab")
        || normalized.contains("/etc/cron.d/")
        || normalized.contains("/var/spool/cron/crontabs/")
    {
        extract_crontab(candidate, bytes, &mut outcome);
    } else if normalized.ends_with("/var/log/auth.log") || normalized.ends_with("/var/log/secure") {
        extract_sudo_log(candidate, bytes, &mut outcome);
    } else {
        outcome.warnings.push(format!(
            "{} 未被识别为已支持的 Linux 痕迹类型",
            candidate.path
        ));
    }

    outcome
}

fn extract_journal(candidate: &EvidenceCandidate, bytes: &[u8], outcome: &mut ExtractionOutcome) {
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
                                .map(|(k, v)| (k, Value::String(v)))
                                .collect(),
                        ),
                    );
                }

                if let Some(ts) = entry.timestamp {
                    attrs.insert("timestamp".to_string(), Value::String(ts.to_rfc3339()));
                }

                let title = entry
                    .message
                    .as_deref()
                    .map(|m| format!("Journal: {}", truncate(m, 80)))
                    .unwrap_or_else(|| "Journal entry".to_string());
                outcome.artifacts.push(make_artifact(
                    "LinuxJournal",
                    title.clone(),
                    title,
                    candidate,
                    "linux.journal",
                    attrs.clone(),
                ));

                if let Some(ts) = entry.timestamp {
                    outcome.timeline_events.push(make_timeline_event(
                        &candidate.file_id,
                        "linux.journal",
                        ts,
                        "Journal message".to_string(),
                        entry.message.unwrap_or_default(),
                        attrs,
                        "linux.journal",
                    ));
                }
            }
        }
        Err(err) => outcome
            .warnings
            .push(format!("{} journal parse failed: {}", candidate.path, err)),
    }
}

fn extract_wtmp(candidate: &EvidenceCandidate, bytes: &[u8], outcome: &mut ExtractionOutcome) {
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
                if let Some(ts) = record.login_time {
                    attrs.insert("loginTime".to_string(), Value::String(ts.to_rfc3339()));
                }
                if let Some(ts) = record.logout_time {
                    attrs.insert("logoutTime".to_string(), Value::String(ts.to_rfc3339()));
                }

                let (event_type, title) = wtmp_event_title(&record);
                outcome.artifacts.push(make_artifact(
                    "LinuxWtmp",
                    title.clone(),
                    title,
                    candidate,
                    "linux.wtmp",
                    attrs.clone(),
                ));

                if let Some(ts) = record.login_time {
                    outcome.timeline_events.push(make_timeline_event(
                        &candidate.file_id,
                        event_type,
                        ts,
                        format!(
                            "{} {}@{} ({})",
                            event_type, record.user, record.host, record.terminal
                        ),
                        format!("PID {} record_type {}", record.pid, record.record_type),
                        attrs.clone(),
                        "linux.wtmp",
                    ));
                }
                if let Some(ts) = record.logout_time {
                    outcome.timeline_events.push(make_timeline_event(
                        &candidate.file_id,
                        "logout",
                        ts,
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
        Err(err) => outcome
            .warnings
            .push(format!("{} wtmp parse failed: {}", candidate.path, err)),
    }
}

fn wtmp_event_title(record: &artifacts_linux::LoginRecord) -> (&'static str, String) {
    match record.record_type {
        2 => (
            "boot",
            format!(
                "Boot: {}",
                record
                    .login_time
                    .map(|t| t.to_rfc3339())
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
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_default()
            ),
        ),
    }
}

fn extract_bash_history(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_bash_history(&text) {
        Ok(commands) => {
            for cmd in commands {
                let mut attrs = base_attrs(candidate);
                attrs.insert("command".to_string(), Value::String(cmd.command.clone()));
                attrs.insert(
                    "lineNumber".to_string(),
                    Value::Number(cmd.line_number.into()),
                );
                if let Some(ts) = cmd.timestamp {
                    attrs.insert("timestamp".to_string(), Value::String(ts.to_rfc3339()));
                }

                outcome.artifacts.push(make_artifact(
                    "LinuxBashCommand",
                    format!("Bash: {}", truncate(&cmd.command, 80)),
                    cmd.command.clone(),
                    candidate,
                    "linux.bash_history",
                    attrs.clone(),
                ));

                if let Some(ts) = cmd.timestamp {
                    outcome.timeline_events.push(make_timeline_event(
                        &candidate.file_id,
                        "linux.bash_command",
                        ts,
                        format!("Bash command: {}", truncate(&cmd.command, 80)),
                        cmd.command,
                        attrs,
                        "linux.bash_history",
                    ));
                }
            }
        }
        Err(err) => outcome.warnings.push(format!(
            "{} bash history parse failed: {}",
            candidate.path, err
        )),
    }
}

fn extract_apt_history(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_apt_history(&text) {
        Ok(events) => {
            for event in events {
                push_apt_event(candidate, event, outcome);
            }
        }
        Err(err) => outcome.warnings.push(format!(
            "{} APT history parse failed: {}",
            candidate.path, err
        )),
    }
}

fn extract_dpkg_log(candidate: &EvidenceCandidate, bytes: &[u8], outcome: &mut ExtractionOutcome) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_dpkg_log(&text) {
        Ok(events) => {
            for event in events {
                push_apt_event(candidate, event, outcome);
            }
        }
        Err(err) => outcome
            .warnings
            .push(format!("{} dpkg log parse failed: {}", candidate.path, err)),
    }
}

fn push_apt_event(
    candidate: &EvidenceCandidate,
    event: artifacts_linux::AptEvent,
    outcome: &mut ExtractionOutcome,
) {
    let mut attrs = base_attrs(candidate);
    attrs.insert("action".to_string(), Value::String(event.action.clone()));
    attrs.insert("package".to_string(), Value::String(event.package.clone()));
    attrs.insert("version".to_string(), Value::String(event.version.clone()));
    if let Some(ts) = event.timestamp {
        attrs.insert("timestamp".to_string(), Value::String(ts.to_rfc3339()));
    }

    outcome.artifacts.push(make_artifact(
        "LinuxAptEvent",
        format!("APT/DPKG {} {}", event.action, event.package),
        format!(
            "{} {} {} ({})",
            event.action,
            event.package,
            event.version,
            event.timestamp.map(|t| t.to_rfc3339()).unwrap_or_default()
        ),
        candidate,
        "linux.apt",
        attrs.clone(),
    ));

    if let Some(ts) = event.timestamp {
        outcome.timeline_events.push(make_timeline_event(
            &candidate.file_id,
            "linux.apt",
            ts,
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

fn extract_crontab(candidate: &EvidenceCandidate, bytes: &[u8], outcome: &mut ExtractionOutcome) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::cron::parse_crontab_with_source(&text, &candidate.path) {
        Ok(jobs) => {
            for job in jobs {
                let mut attrs = base_attrs(candidate);
                attrs.insert("schedule".to_string(), Value::String(job.schedule.clone()));
                attrs.insert("command".to_string(), Value::String(job.command.clone()));
                attrs.insert(
                    "sourceFile".to_string(),
                    Value::String(job.source_file.clone()),
                );
                if let Some(user) = job.user {
                    attrs.insert("user".to_string(), Value::String(user));
                }

                outcome.artifacts.push(make_artifact(
                    "LinuxCronJob",
                    format!("Cron: {}", truncate(&job.command, 80)),
                    format!("{} runs `{}`", job.schedule, job.command),
                    candidate,
                    "linux.crontab",
                    attrs,
                ));
            }
        }
        Err(err) => outcome
            .warnings
            .push(format!("{} crontab parse failed: {}", candidate.path, err)),
    }
}

fn extract_sudo_log(candidate: &EvidenceCandidate, bytes: &[u8], outcome: &mut ExtractionOutcome) {
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
                if let Some(ts) = event.timestamp {
                    attrs.insert("timestamp".to_string(), Value::String(ts.to_rfc3339()));
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

                if let Some(ts) = event.timestamp {
                    outcome.timeline_events.push(make_timeline_event(
                        &candidate.file_id,
                        "linux.sudo",
                        ts,
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
        Err(err) => outcome
            .warnings
            .push(format!("{} sudo log parse failed: {}", candidate.path, err)),
    }
}

fn insert_opt(attrs: &mut BTreeMap<String, Value>, key: &str, value: Option<String>) {
    if let Some(v) = value {
        if !v.is_empty() {
            attrs.insert(key.to_string(), Value::String(v));
        }
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max_len).collect::<String>())
    }
}
