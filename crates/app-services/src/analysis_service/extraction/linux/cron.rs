use super::common::truncate;
use crate::analysis_service::artifact_builders::{base_attrs, make_artifact};
use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};
use crate::analysis_service::extraction::ExtractionOutcome;
use artifacts_linux::cron::CrontabKind;
use serde_json::Value;

pub(in crate::analysis_service::extraction) fn is_cron_path(normalized: &str) -> bool {
    normalized.ends_with("/etc/crontab")
        || normalized.contains("/etc/cron.d/")
        || normalized.contains("/etc/cron.daily/")
        || normalized.contains("/etc/cron.hourly/")
        || normalized.contains("/etc/cron.monthly/")
        || normalized.contains("/etc/cron.weekly/")
        || is_spool_user_crontab(normalized)
}

/// `/var/spool/cron/<user>` (RHEL) and `/var/spool/cron/crontabs/<user>`
/// (Debian) hold per-user crontabs as direct files. Subdirectories such as
/// `atjobs/` or `anacron/` are not crontabs and stay unrouted.
fn is_spool_user_crontab(normalized: &str) -> bool {
    let Some((_, rest)) = normalized.split_once("/var/spool/cron/") else {
        return false;
    };
    let rest = rest.strip_prefix("crontabs/").unwrap_or(rest);
    !rest.is_empty() && !rest.contains('/')
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
        Err(error) => outcome.warnings.push(format!(
            "{} crontab parse failed: {}",
            candidate.path, error
        )),
    }
}
