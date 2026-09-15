use super::common::{cap_source_events, MAX_LOGIN_EVENTS_PER_SOURCE};
use crate::analysis_service::artifact_builders::{base_attrs, make_artifact, make_timeline_event};
use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};
use crate::analysis_service::extraction::ExtractionOutcome;
use serde_json::Value;
use std::collections::BTreeMap;

pub(in crate::analysis_service::extraction) fn is_wtmp_path(normalized: &str) -> bool {
    normalized.ends_with("/var/log/wtmp")
        || normalized.contains("/var/log/wtmp.")
        || normalized.ends_with("/var/log/btmp")
        || normalized.contains("/var/log/btmp.")
        || normalized.ends_with("/var/log/utmp")
        || normalized.ends_with("/run/utmp")
}

/// `/var/log/btmp*` shares the utmp record format but logs *failed* login
/// attempts (the lastb(8) source), never successful sessions.
fn is_btmp_path(normalized: &str) -> bool {
    normalized.ends_with("/var/log/btmp") || normalized.contains("/var/log/btmp.")
}

pub(in crate::analysis_service::extraction) fn is_lastlog_path(normalized: &str) -> bool {
    normalized.ends_with("/var/log/lastlog")
}

pub(in crate::analysis_service::extraction) fn is_faillog_path(normalized: &str) -> bool {
    normalized.ends_with("/var/log/faillog")
}

/// Extract wtmp/utmp/btmp login records.
///
/// btmp records are *failed* login attempts, so they are emitted as
/// `login_failed` timeline events titled "Failed login …" with
/// `recordKind = "btmp"`, and no logout event is synthesized (a failed
/// attempt has no session to close). Each btmp artifact also carries
/// `userFailureCount`, the per-user attempt total within that btmp source,
/// as the btmp-side failure counter.
///
/// Cross-count boundary: a direct btmp↔faillog reconciliation is not done at
/// this layer. Extraction is per-file with no cross-candidate state, and
/// faillog keys counters by UID while btmp keys by username; the UID↔name
/// mapping (passwd) is unavailable here. Consumers compare `userFailureCount`
/// against faillog `failures` per user after resolving UIDs in the report
/// layer.
pub(super) fn extract(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let is_btmp = is_btmp_path(&normalize_evidence_path(&candidate.path));
    match artifacts_linux::parse_wtmp(bytes) {
        Ok(records) => {
            let failure_totals = if is_btmp {
                failure_totals_by_user(&records)
            } else {
                BTreeMap::new()
            };
            let records = cap_source_events(
                candidate,
                if is_btmp { "btmp" } else { "wtmp" },
                MAX_LOGIN_EVENTS_PER_SOURCE,
                records,
                &mut outcome.warnings,
            );
            for record in &records {
                let user_failure_total = failure_totals.get(record.user.as_str()).copied();
                push_record_outputs(candidate, record, is_btmp, user_failure_total, outcome);
            }
        }
        Err(error) => outcome
            .warnings
            .push(format!("{} wtmp parse failed: {}", candidate.path, error)),
    }
}

/// Failed-attempt totals per user within one parsed btmp source; boot and
/// runlevel markers (ut_type 2/1) are not login attempts and are excluded.
fn failure_totals_by_user(records: &[artifacts_linux::LoginRecord]) -> BTreeMap<String, usize> {
    let mut totals: BTreeMap<String, usize> = BTreeMap::new();
    for record in records
        .iter()
        .filter(|record| !matches!(record.record_type, 1 | 2))
    {
        *totals.entry(record.user.clone()).or_insert(0) += 1;
    }
    totals
}

fn push_record_outputs(
    candidate: &EvidenceCandidate,
    record: &artifacts_linux::LoginRecord,
    is_btmp: bool,
    user_failure_total: Option<usize>,
    outcome: &mut ExtractionOutcome,
) {
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
    if is_btmp {
        attrs.insert("recordKind".to_string(), Value::String("btmp".to_string()));
    }
    if let Some(total) = user_failure_total {
        attrs.insert("userFailureCount".to_string(), Value::Number(total.into()));
    }
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

    let (event_type, title) = event_title(record, is_btmp);
    outcome.artifacts.push(make_artifact(
        "LinuxWtmp",
        title.clone(),
        title,
        candidate,
        "linux.wtmp",
        attrs.clone(),
    ));

    if let Some(timestamp) = record.login_time {
        let title = if is_btmp {
            format!(
                "Failed login {}@{} ({})",
                record.user, record.host, record.terminal
            )
        } else {
            format!(
                "{} {}@{} ({})",
                event_type, record.user, record.host, record.terminal
            )
        };
        outcome.timeline_events.push(make_timeline_event(
            &candidate.file_id,
            event_type,
            timestamp,
            title,
            format!("PID {} record_type {}", record.pid, record.record_type),
            attrs.clone(),
            "linux.wtmp",
        ));
    }
    // A btmp attempt has no session to close, so no logout event is emitted
    // even when a DEAD_PROCESS paired a logout_time onto the record.
    let logout_time = if is_btmp { None } else { record.logout_time };
    if let Some(timestamp) = logout_time {
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

fn event_title(record: &artifacts_linux::LoginRecord, is_btmp: bool) -> (&'static str, String) {
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
        _ if is_btmp => (
            "login_failed",
            format!(
                "Failed login {}@{} via {} ({})",
                record.user,
                record.host,
                record.terminal,
                record
                    .login_time
                    .map(|timestamp| timestamp.to_rfc3339())
                    .unwrap_or_default()
            ),
        ),
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

/// Extract `/var/log/lastlog` per-UID last-login records.
///
/// Artifact-type tradeoff: lastlog records reuse the `LinuxWtmp` family (the
/// login family) instead of a new `LinuxLastlog` type. A new type would
/// require a new transport DTO, summary mapping, frontend model + panel, and
/// governance-catalog entries, while the existing `LinuxLoginRecordDto`
/// already surfaces the fields that matter (terminal, host, loginTime). The
/// UID rides in the `uid` attribute and `recordKind = "lastlog"` keeps the
/// source unambiguous. Usernames are not resolved here — that needs a passwd
/// cross-reference the extraction layer does not have; the UID is the
/// authoritative key.
pub(super) fn extract_lastlog(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    match artifacts_linux::parse_lastlog(bytes) {
        Ok(records) => {
            let records = cap_source_events(
                candidate,
                "lastlog",
                MAX_LOGIN_EVENTS_PER_SOURCE,
                records,
                &mut outcome.warnings,
            );
            for record in records {
                let mut attrs = base_attrs(candidate);
                attrs.insert(
                    "recordKind".to_string(),
                    Value::String("lastlog".to_string()),
                );
                attrs.insert("uid".to_string(), Value::Number(record.uid.into()));
                attrs.insert("terminal".to_string(), Value::String(record.line.clone()));
                attrs.insert("host".to_string(), Value::String(record.host.clone()));
                if let Some(timestamp) = record.time {
                    attrs.insert(
                        "loginTime".to_string(),
                        Value::String(timestamp.to_rfc3339()),
                    );
                }

                let title = format!(
                    "Last login (lastlog): uid {} on {} from {}",
                    record.uid,
                    record.line,
                    if record.host.is_empty() {
                        "local"
                    } else {
                        record.host.as_str()
                    }
                );
                outcome.artifacts.push(make_artifact(
                    "LinuxWtmp",
                    title.clone(),
                    title,
                    candidate,
                    "linux.lastlog",
                    attrs.clone(),
                ));

                if let Some(timestamp) = record.time {
                    outcome.timeline_events.push(make_timeline_event(
                        &candidate.file_id,
                        "login",
                        timestamp,
                        format!(
                            "Last login uid {}@{} ({})",
                            record.uid, record.host, record.line
                        ),
                        format!("lastlog record for uid {}", record.uid),
                        attrs,
                        "linux.lastlog",
                    ));
                }
            }
        }
        Err(error) => outcome.warnings.push(format!(
            "{} lastlog parse failed: {}",
            candidate.path, error
        )),
    }
}

/// Extract `/var/log/faillog` per-UID login-failure counters.
///
/// Same tradeoff as `extract_lastlog`: records reuse the `LinuxWtmp` login
/// family with `recordKind = "faillog"`; the failure counters
/// (`failures`/`failMax`/`locktimeSeconds`/`lockout`) live in the artifact
/// attributes. faillog carries no host field, so `host` stays empty and
/// `loginTime` holds the most recent *failure* time.
pub(super) fn extract_faillog(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    match artifacts_linux::parse_faillog(bytes) {
        Ok(records) => {
            let records = cap_source_events(
                candidate,
                "faillog",
                MAX_LOGIN_EVENTS_PER_SOURCE,
                records,
                &mut outcome.warnings,
            );
            for record in records {
                let mut attrs = base_attrs(candidate);
                attrs.insert(
                    "recordKind".to_string(),
                    Value::String("faillog".to_string()),
                );
                attrs.insert("uid".to_string(), Value::Number(record.uid.into()));
                attrs.insert("terminal".to_string(), Value::String(record.line.clone()));
                attrs.insert(
                    "failures".to_string(),
                    Value::Number(record.failure_count.into()),
                );
                attrs.insert(
                    "failMax".to_string(),
                    Value::Number(record.max_failures.into()),
                );
                attrs.insert(
                    "locktimeSeconds".to_string(),
                    Value::Number(record.locktime_seconds.into()),
                );
                attrs.insert("lockout".to_string(), Value::Bool(record.lockout));
                if let Some(timestamp) = record.last_failure {
                    attrs.insert(
                        "loginTime".to_string(),
                        Value::String(timestamp.to_rfc3339()),
                    );
                }

                let title = format!(
                    "Failed logins (faillog): uid {} count {} on {}",
                    record.uid, record.failure_count, record.line
                );
                outcome.artifacts.push(make_artifact(
                    "LinuxWtmp",
                    title.clone(),
                    title,
                    candidate,
                    "linux.faillog",
                    attrs.clone(),
                ));

                if let Some(timestamp) = record.last_failure {
                    outcome.timeline_events.push(make_timeline_event(
                        &candidate.file_id,
                        "login_failed",
                        timestamp,
                        format!(
                            "Failed login uid {} ({}) count {}",
                            record.uid, record.line, record.failure_count
                        ),
                        format!("faillog record for uid {}", record.uid),
                        attrs,
                        "linux.faillog",
                    ));
                }
            }
        }
        Err(error) => outcome.warnings.push(format!(
            "{} faillog parse failed: {}",
            candidate.path, error
        )),
    }
}
