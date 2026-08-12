use super::common::{cap_source_events, truncate, MAX_SHELL_HISTORY_EVENTS_PER_SOURCE};
use crate::analysis_service::artifact_builders::{base_attrs, make_artifact, make_timeline_event};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::extraction::ExtractionOutcome;
use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;

pub(in crate::analysis_service::extraction) fn is_bash_history_path(normalized: &str) -> bool {
    normalized.ends_with(".bash_history")
}

pub(in crate::analysis_service::extraction) fn is_zsh_history_path(normalized: &str) -> bool {
    normalized.ends_with(".zsh_history")
}

pub(in crate::analysis_service::extraction) fn is_fish_history_path(normalized: &str) -> bool {
    normalized.ends_with(".fish_history") || normalized.contains("/.local/share/fish/fish_history")
}

pub(in crate::analysis_service::extraction) fn is_plain_shell_history_path(
    normalized: &str,
) -> bool {
    normalized.ends_with(".python_history")
}

pub(super) fn extract_bash(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_bash_history(&text) {
        Ok(commands) => {
            let commands = cap_source_events(
                candidate,
                "bash history",
                MAX_SHELL_HISTORY_EVENTS_PER_SOURCE,
                commands,
                &mut outcome.warnings,
            );
            for command in commands {
                let mut attrs = base_attrs(candidate);
                attrs.insert(
                    "command".to_string(),
                    Value::String(command.command.clone()),
                );
                attrs.insert(
                    "lineNumber".to_string(),
                    Value::Number(command.line_number.into()),
                );
                if let Some(timestamp) = command.timestamp {
                    attrs.insert(
                        "timestamp".to_string(),
                        Value::String(timestamp.to_rfc3339()),
                    );
                }

                outcome.artifacts.push(make_artifact(
                    "LinuxBashCommand",
                    format!("Bash: {}", truncate(&command.command, 80)),
                    command.command.clone(),
                    candidate,
                    "linux.bash_history",
                    attrs.clone(),
                ));

                if let Some(timestamp) = command.timestamp {
                    outcome.timeline_events.push(make_timeline_event(
                        &candidate.file_id,
                        "linux.bash_command",
                        timestamp,
                        format!("Bash command: {}", truncate(&command.command, 80)),
                        command.command,
                        attrs,
                        "linux.bash_history",
                    ));
                }
            }
        }
        Err(error) => outcome.warnings.push(format!(
            "{} bash history parse failed: {}",
            candidate.path, error
        )),
    }
}

pub(super) fn extract_zsh(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    let mut commands = Vec::new();
    for (line_number, line) in (1u64..).zip(text.lines()) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (timestamp, command) = parse_zsh_history_line(trimmed);
        if command.is_empty() {
            continue;
        }
        commands.push(ShellHistoryCommand {
            command,
            timestamp,
            line_number,
            shell: "zsh",
        });
    }
    push_commands(candidate, commands, "linux.zsh_history", outcome);
}

pub(super) fn extract_fish(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    let mut commands = Vec::new();
    let mut current_command: Option<(u64, String)> = None;
    let mut current_timestamp: Option<DateTime<Utc>> = None;

    for (line_number, line) in (1u64..).zip(text.lines()) {
        let trimmed = line.trim();
        if let Some(command) = trimmed.strip_prefix("- cmd:") {
            if let Some((previous_line, previous_command)) = current_command.take() {
                commands.push(ShellHistoryCommand {
                    command: previous_command,
                    timestamp: current_timestamp.take(),
                    line_number: previous_line,
                    shell: "fish",
                });
            }
            current_command = Some((line_number, unescape_fish_value(command.trim())));
        } else if let Some(when) = trimmed.strip_prefix("when:") {
            current_timestamp = when
                .trim()
                .parse::<i64>()
                .ok()
                .and_then(|epoch| Utc.timestamp_opt(epoch, 0).single());
        }
    }

    if let Some((line_number, command)) = current_command {
        commands.push(ShellHistoryCommand {
            command,
            timestamp: current_timestamp,
            line_number,
            shell: "fish",
        });
    }
    push_commands(candidate, commands, "linux.fish_history", outcome);
}

pub(super) fn extract_plain(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    let commands = text
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let command = line.trim();
            if command.is_empty() || command.starts_with('#') {
                return None;
            }
            Some(ShellHistoryCommand {
                command: command.to_string(),
                timestamp: None,
                line_number: index as u64 + 1,
                shell: "shell",
            })
        })
        .collect::<Vec<_>>();
    push_commands(candidate, commands, "linux.shell_history", outcome);
}

struct ShellHistoryCommand {
    command: String,
    timestamp: Option<DateTime<Utc>>,
    line_number: u64,
    shell: &'static str,
}

fn push_commands(
    candidate: &EvidenceCandidate,
    commands: Vec<ShellHistoryCommand>,
    parser: &str,
    outcome: &mut ExtractionOutcome,
) {
    let commands = cap_source_events(
        candidate,
        "shell history",
        MAX_SHELL_HISTORY_EVENTS_PER_SOURCE,
        commands,
        &mut outcome.warnings,
    );
    for command in commands {
        let mut attrs = base_attrs(candidate);
        attrs.insert(
            "command".to_string(),
            Value::String(command.command.clone()),
        );
        attrs.insert(
            "lineNumber".to_string(),
            Value::Number(command.line_number.into()),
        );
        attrs.insert(
            "shell".to_string(),
            Value::String(command.shell.to_string()),
        );
        if let Some(timestamp) = command.timestamp {
            attrs.insert(
                "timestamp".to_string(),
                Value::String(timestamp.to_rfc3339()),
            );
        }

        outcome.artifacts.push(make_artifact(
            "LinuxBashCommand",
            format!("{}: {}", command.shell, truncate(&command.command, 80)),
            command.command.clone(),
            candidate,
            parser,
            attrs.clone(),
        ));

        if let Some(timestamp) = command.timestamp {
            outcome.timeline_events.push(make_timeline_event(
                &candidate.file_id,
                "linux.shell_command",
                timestamp,
                format!(
                    "{} command: {}",
                    command.shell,
                    truncate(&command.command, 80)
                ),
                command.command,
                attrs,
                parser,
            ));
        }
    }
}

fn parse_zsh_history_line(line: &str) -> (Option<DateTime<Utc>>, String) {
    if let Some(rest) = line.strip_prefix(": ") {
        let mut parts = rest.splitn(2, ';');
        if let (Some(metadata), Some(command)) = (parts.next(), parts.next()) {
            let timestamp = metadata
                .split(':')
                .next()
                .and_then(|raw| raw.trim().parse::<i64>().ok())
                .and_then(|value| Utc.timestamp_opt(value, 0).single());
            return (timestamp, command.trim().to_string());
        }
    }
    (None, line.trim().to_string())
}

fn unescape_fish_value(value: &str) -> String {
    value
        .replace("\\n", "\n")
        .replace("\\:", ":")
        .replace("\\\\", "\\")
}
