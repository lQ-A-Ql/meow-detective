use super::common::truncate;
use crate::analysis_service::artifact_builders::{base_attrs, make_artifact};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::extraction::ExtractionOutcome;
use serde_json::Value;

pub(in crate::analysis_service::extraction) fn is_cron_path(normalized: &str) -> bool {
    normalized.ends_with("/etc/crontab")
        || normalized.contains("/etc/cron.d/")
        || normalized.contains("/etc/cron.daily/")
        || normalized.contains("/etc/cron.hourly/")
        || normalized.contains("/etc/cron.monthly/")
        || normalized.contains("/etc/cron.weekly/")
        || normalized.contains("/var/spool/cron/")
}

pub(super) fn extract(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
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
        Err(error) => outcome.warnings.push(format!(
            "{} crontab parse failed: {}",
            candidate.path, error
        )),
    }
}
