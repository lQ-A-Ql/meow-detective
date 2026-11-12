use super::common::truncate;
use crate::analysis_service::artifact_builders::{base_attrs, make_artifact};
use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};
use crate::analysis_service::extraction::ExtractionOutcome;
use artifacts_linux::cron::CrontabKind;
use serde_json::Value;

pub(in crate::analysis_service::extraction) fn is_cron_path(normalized: &str) -> bool {
    // `/etc/cron.{hourly,daily,weekly,monthly}/` hold executable *scripts*,
    // not crontabs; they route to the generic text-config extractor instead
    // (see `is_system_config_path`) so shell lines are never split into fake
    // schedules and commands.
    normalized.ends_with("/etc/crontab")
        || normalized.contains("/etc/cron.d/")
        || is_spool_user_crontab(normalized)
}

/// `/var/spool/cron/<user>` (RHEL) and `/var/spool/cron/crontabs/<user>`
/// (Debian) hold per-user crontabs as direct files. Subdirectories such as
/// `atjobs/` or `anacron/` are not crontabs and stay unrouted.
fn is_spool_user_crontab(normalized: &str) -> bool {
    spool_file_owner(normalized).is_some()
}

/// The owning account of a user crontab is the spool file name itself.
fn spool_file_owner(normalized: &str) -> Option<&str> {
    let (_, rest) = normalized.split_once("/var/spool/cron/")?;
    let rest = rest.strip_prefix("crontabs/").unwrap_or(rest);
    if rest.is_empty() || rest.contains('/') {
        None
    } else {
        Some(rest)
    }
}

fn crontab_kind(normalized: &str) -> CrontabKind {
    if normalized.contains("/var/spool/cron/") {
        CrontabKind::User
    } else {
        CrontabKind::System
    }
}

pub(super) fn extract(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    let normalized = normalize_evidence_path(&candidate.path);
    let kind = crontab_kind(&normalized);
    // User crontabs carry no username column; the owning account is the spool
    // file name itself (`/var/spool/cron/root` -> `root`).
    let spool_owner = if kind == CrontabKind::User {
        spool_file_owner(&normalized)
    } else {
        None
    };
    match artifacts_linux::cron::parse_crontab_with_source_and_kind(&text, &candidate.path, kind) {
        Ok(jobs) => {
            for job in jobs {
                let mut attrs = base_attrs(candidate);
                attrs.insert("schedule".to_string(), Value::String(job.schedule.clone()));
                attrs.insert("command".to_string(), Value::String(job.command.clone()));
                attrs.insert(
                    "sourceFile".to_string(),
                    Value::String(job.source_file.clone()),
                );
                if let Some(user) = job.user.as_deref().or(spool_owner) {
                    attrs.insert("user".to_string(), Value::String(user.to_string()));
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
        Err(error) => outcome.warnings.push(format!(
            "{} crontab parse failed: {}",
            candidate.path, error
        )),
    }
}
