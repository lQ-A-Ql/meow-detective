use crate::analysis_service::artifact_builders::{base_attrs, make_artifact, make_timeline_event};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::extraction::ExtractionOutcome;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::BTreeMap;

/// Derive the profile username from an NTUSER.DAT path.
///
/// The hive normally sits directly under the user profile directory, so the
/// segment immediately preceding `NTUSER.DAT` is treated as the username.
fn subject_username(candidate: &EvidenceCandidate) -> Option<String> {
    let parts: Vec<&str> = candidate.path.split(['/', '\\']).collect();
    parts
        .iter()
        .position(|segment| segment.eq_ignore_ascii_case("ntuser.dat"))
        .and_then(|idx| idx.checked_sub(1))
        .and_then(|idx| parts.get(idx).copied())
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string())
}

fn add_subject_username(attrs: &mut BTreeMap<String, Value>, candidate: &EvidenceCandidate) {
    if let Some(username) = subject_username(candidate) {
        attrs.insert("subjectUsername".to_string(), Value::String(username));
    }
}

pub(super) fn user_assist_artifacts(
    candidate: &EvidenceCandidate,
    info: &artifacts_windows::NtuserInfo,
) -> ExtractionOutcome {
    let mut outcome = ExtractionOutcome::default();
    for entry in &info.ua_entries {
        let mut attrs = base_attrs(candidate);
        add_subject_username(&mut attrs, candidate);
        attrs.insert(
            "programPath".to_string(),
            Value::String(entry.executable_path.clone()),
        );
        attrs.insert(
            "execCount".to_string(),
            Value::Number(entry.run_count.into()),
        );
        if let Some(ts) = entry.last_run.as_ref().filter(|s| !s.is_empty()) {
            attrs.insert("lastExecTime".to_string(), Value::String(ts.clone()));
        }
        attrs.insert(
            "focusTimeMs".to_string(),
            Value::Number(entry.focus_time_ms.into()),
        );
        attrs.insert(
            "sessionId".to_string(),
            Value::Number(entry.session_id.into()),
        );
        attrs.insert(
            "parser".to_string(),
            Value::String("registry.ntuser".to_string()),
        );
        attrs.insert(
            "executionEvidence".to_string(),
            Value::String("registry.user_assist".to_string()),
        );
        attrs.insert(
            "timestampSemantics".to_string(),
            Value::String("UserAssist last execution timestamp".to_string()),
        );

        if let Some(ts_str) = entry.last_run.as_ref().filter(|s| !s.is_empty()) {
            if let Ok(ts) = DateTime::parse_from_rfc3339(ts_str) {
                let ts_utc = ts.with_timezone(&Utc);
                outcome.timeline_events.push(make_timeline_event(
                    &candidate.file_id,
                    "FILE_EXECUTED",
                    ts_utc,
                    format!("UserAssist execution: {}", entry.executable_path),
                    format!(
                        "{} executed {} times (focus {} ms)",
                        entry.executable_path, entry.run_count, entry.focus_time_ms
                    ),
                    attrs.clone(),
                    "registry.ntuser.v1",
                ));
            }
        }

        outcome.artifacts.push(make_artifact(
            "RegistryUserAssist",
            format!("UserAssist: {}", entry.executable_path),
            format!(
                "Executed {} times (focus {} ms)",
                entry.run_count, entry.focus_time_ms
            ),
            candidate,
            "registry.ntuser.v1",
            attrs,
        ));
    }
    outcome
}

pub(super) fn default_browser_artifacts(
    candidate: &EvidenceCandidate,
    info: &artifacts_windows::NtuserInfo,
) -> Vec<domain::Artifact> {
    let Some(prog_id) = info.default_browser.as_ref().filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    let display_name = super::shared::friendly_browser_name(prog_id);
    let mut attrs = base_attrs(candidate);
    add_subject_username(&mut attrs, candidate);
    attrs.insert(
        "displayName".to_string(),
        Value::String(display_name.to_string()),
    );
    attrs.insert("version".to_string(), Value::String(String::new()));
    attrs.insert("publisher".to_string(), Value::String(String::new()));
    attrs.insert("progId".to_string(), Value::String(prog_id.clone()));
    attrs.insert(
        "parser".to_string(),
        Value::String("registry.ntuser.browser".to_string()),
    );
    vec![make_artifact(
        "RegistryInstalledSoftware",
        format!("Installed Software: {display_name}"),
        format!("Default browser ProgId: {prog_id}"),
        candidate,
        "registry.ntuser.browser.v1",
        attrs,
    )]
}

pub(super) fn open_save_mru_artifacts(
    candidate: &EvidenceCandidate,
    info: &artifacts_windows::NtuserInfo,
) -> Vec<domain::Artifact> {
    info.open_save_mru
        .iter()
        .map(|entry| {
            let mut attrs = base_attrs(candidate);
            add_subject_username(&mut attrs, candidate);
            attrs.insert(
                "extension".to_string(),
                Value::String(entry.extension.clone()),
            );
            attrs.insert(
                "valueName".to_string(),
                Value::String(entry.value_name.clone()),
            );
            attrs.insert(
                "fileName".to_string(),
                Value::String(entry.file_name.clone()),
            );
            attrs.insert(
                "rawPidlHex".to_string(),
                Value::String(entry.raw_pidl_hex.clone()),
            );
            attrs.insert(
                "sourceKeyPath".to_string(),
                Value::String(entry.source_key_path.clone()),
            );
            if let Some(ts) = entry.last_write.as_ref() {
                attrs.insert("lastWrite".to_string(), Value::String(ts.clone()));
            }
            attrs.insert(
                "parser".to_string(),
                Value::String("registry.ntuser.opensave".to_string()),
            );
            make_artifact(
                "RegistryOpenSaveMru",
                format!("OpenSave MRU: {}", entry.file_name),
                format!(
                    "{} (extension {}, value {})",
                    entry.file_name, entry.extension, entry.value_name
                ),
                candidate,
                "registry.ntuser.opensave.v1",
                attrs,
            )
        })
        .collect()
}

pub(super) fn last_visited_mru_artifacts(
    candidate: &EvidenceCandidate,
    info: &artifacts_windows::NtuserInfo,
) -> Vec<domain::Artifact> {
    info.last_visited_mru
        .iter()
        .map(|entry| {
            let mut attrs = base_attrs(candidate);
            add_subject_username(&mut attrs, candidate);
            attrs.insert(
                "valueName".to_string(),
                Value::String(entry.value_name.clone()),
            );
            attrs.insert("path".to_string(), Value::String(entry.path.clone()));
            attrs.insert(
                "rawPidlHex".to_string(),
                Value::String(entry.raw_pidl_hex.clone()),
            );
            attrs.insert(
                "sourceKeyPath".to_string(),
                Value::String(entry.source_key_path.clone()),
            );
            if let Some(ts) = entry.last_write.as_ref() {
                attrs.insert("lastWrite".to_string(), Value::String(ts.clone()));
            }
            attrs.insert(
                "parser".to_string(),
                Value::String("registry.ntuser.lastvisited".to_string()),
            );
            make_artifact(
                "RegistryLastVisitedMru",
                format!("Last Visited MRU: {}", entry.path),
                format!("{} (value {})", entry.path, entry.value_name),
                candidate,
                "registry.ntuser.lastvisited.v1",
                attrs,
            )
        })
        .collect()
}

pub(super) fn run_mru_artifacts(
    candidate: &EvidenceCandidate,
    info: &artifacts_windows::NtuserInfo,
) -> Vec<domain::Artifact> {
    info.run_mru
        .iter()
        .map(|entry| {
            let mut attrs = base_attrs(candidate);
            add_subject_username(&mut attrs, candidate);
            attrs.insert(
                "valueName".to_string(),
                Value::String(entry.value_name.clone()),
            );
            attrs.insert("command".to_string(), Value::String(entry.command.clone()));
            attrs.insert(
                "sourceKeyPath".to_string(),
                Value::String(entry.source_key_path.clone()),
            );
            if let Some(ts) = entry.last_write.as_ref() {
                attrs.insert("lastWrite".to_string(), Value::String(ts.clone()));
            }
            attrs.insert(
                "parser".to_string(),
                Value::String("registry.ntuser.runmru".to_string()),
            );
            make_artifact(
                "RegistryRunMru",
                format!("Run MRU: {}", entry.command),
                format!("{} = {}", entry.value_name, entry.command),
                candidate,
                "registry.ntuser.runmru.v1",
                attrs,
            )
        })
        .collect()
}
