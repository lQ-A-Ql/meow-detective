mod common;
mod cron;
mod journal;
mod login;
mod mysql;
mod packages;
mod pve;
mod shell_history;
mod sudo;
mod system_config;
mod text_log;
mod timezone;
mod web;

use super::ExtractionOutcome;
use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};

pub(super) use common::{linux_candidate_read_limit, linux_candidate_support};
pub(super) use cron::is_cron_path;
pub(super) use journal::is_journal_path;
pub(super) use login::is_faillog_path;
pub(super) use login::is_lastlog_path;
pub(super) use login::is_wtmp_path;
pub(super) use mysql::{is_mysql_config_path, is_mysql_log_path};
pub(super) use packages::{is_apt_history_path, is_dpkg_log_path, is_rpm_package_log_path};
pub(super) use pve::{is_pve_config_path, is_pve_log_path};
pub(super) use shell_history::{
    is_bash_history_path, is_fish_history_path, is_plain_shell_history_path, is_zsh_history_path,
};
pub(super) use sudo::is_auth_log_path;
pub(super) use system_config::is_ssh_text_path;
pub(super) use system_config::{
    is_init_script_path, is_profile_script_path, is_ssh_candidate_path, is_sudoers_path,
    is_system_config_path, is_systemd_unit_path,
};
pub(super) use text_log::is_text_log_path;
pub(super) use timezone::{resolve_linux_log_time, LinuxLogTimeContext};
pub(super) use web::{
    is_apache_config_path, is_nginx_config_path, is_web_access_log_path, is_web_error_log_path,
    is_web_root_script_path,
};

pub fn extract_linux_candidate(candidate: &EvidenceCandidate, bytes: &[u8]) -> ExtractionOutcome {
    extract_linux_candidate_with_time(candidate, bytes, &LinuxLogTimeContext::utc())
}

/// Extract a Linux candidate, converting naive log timestamps with the
/// inferred host timezone (UTC when no zone could be determined).
pub(super) fn extract_linux_candidate_with_time(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    log_time: &LinuxLogTimeContext,
) -> ExtractionOutcome {
    let mut outcome = ExtractionOutcome::default();
    let normalized = normalize_evidence_path(&candidate.path);
    if crate::analysis_service::artifact_builders::is_docker_overlay_path(&normalized) {
        outcome.warnings.push(format!(
            "{} resides inside a Docker overlay2 layer; extracted records describe container content, not host state",
            candidate.path
        ));
    }
    let decoded;
    let effective_path = normalized.strip_suffix(".gz").unwrap_or(&normalized);
    let input = if normalized.ends_with(".gz") {
        match common::decode_gzip(bytes) {
            Ok((data, truncation)) => {
                decoded = data;
                match truncation {
                    Some(common::GzipTruncation::OutputCap) => outcome.warnings.push(format!(
                        "{} gzip decoded output exceeds the 128 MiB analysis cap; decoded content was truncated before parsing",
                        candidate.path
                    )),
                    Some(common::GzipTruncation::TruncatedStream) => outcome.warnings.push(
                        format!(
                            "{} gzip stream ends prematurely; only the first {} decoded bytes were parsed",
                            candidate.path,
                            decoded.len()
                        ),
                    ),
                    None => {}
                }
                decoded.as_slice()
            }
            Err(error) => {
                outcome
                    .warnings
                    .push(format!("{} gzip decode failed: {}", candidate.path, error));
                return outcome;
            }
        }
    } else {
        bytes
    };

    let source_limit = linux_candidate_read_limit(&normalized);
    if candidate.size > source_limit as u64 {
        outcome.warnings.push(format!(
            "{} exceeds the Linux analysis cap; only the first {} bytes were scanned",
            candidate.path, source_limit
        ));
    }

    dispatch_candidate(effective_path, candidate, input, &mut outcome, log_time);
    outcome
}

fn dispatch_candidate(
    path: &str,
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
    log_time: &LinuxLogTimeContext,
) {
    use super::linux_sections::{linux_artifact_route, LinuxArtifactRouteKind};

    match linux_artifact_route(path).kind {
        LinuxArtifactRouteKind::Journal => journal::extract(candidate, bytes, outcome),
        LinuxArtifactRouteKind::NginxConfig => web::extract_nginx_config(candidate, bytes, outcome),
        LinuxArtifactRouteKind::ApacheConfig => {
            web::extract_apache_config(candidate, bytes, outcome)
        }
        LinuxArtifactRouteKind::WebAccessLog => web::extract_access_log(candidate, bytes, outcome),
        LinuxArtifactRouteKind::WebErrorLog => web::extract_error_log(candidate, bytes, outcome),
        LinuxArtifactRouteKind::WebRootScript => {
            web::extract_root_script(candidate, bytes, outcome)
        }
        LinuxArtifactRouteKind::MysqlConfig => mysql::extract_config(candidate, bytes, outcome),
        LinuxArtifactRouteKind::MysqlLog => mysql::extract_log(candidate, bytes, outcome),
        LinuxArtifactRouteKind::Login => login::extract(candidate, bytes, outcome),
        LinuxArtifactRouteKind::Lastlog => login::extract_lastlog(candidate, bytes, outcome),
        LinuxArtifactRouteKind::Faillog => login::extract_faillog(candidate, bytes, outcome),
        LinuxArtifactRouteKind::BashHistory => {
            shell_history::extract_bash(candidate, bytes, outcome)
        }
        LinuxArtifactRouteKind::ZshHistory => shell_history::extract_zsh(candidate, bytes, outcome),
        LinuxArtifactRouteKind::FishHistory => {
            shell_history::extract_fish(candidate, bytes, outcome)
        }
        LinuxArtifactRouteKind::PlainShellHistory => {
            shell_history::extract_plain(candidate, bytes, outcome)
        }
        LinuxArtifactRouteKind::SystemConfig => system_config::extract(candidate, bytes, outcome),
        LinuxArtifactRouteKind::PveConfig => pve::extract_config(candidate, bytes, outcome),
        LinuxArtifactRouteKind::Sudoers => {
            extract_text_config(candidate, bytes, "linux.sudoers", "sudoers", outcome)
        }
        LinuxArtifactRouteKind::SshConfig => {
            extract_text_config(candidate, bytes, "linux.ssh_config", "sshConfig", outcome)
        }
        LinuxArtifactRouteKind::SystemdUnit => {
            system_config::extract_systemd_unit_config(candidate, bytes, outcome)
        }
        LinuxArtifactRouteKind::InitScript => {
            extract_text_config(candidate, bytes, "linux.init_script", "initScript", outcome)
        }
        LinuxArtifactRouteKind::ProfileScript => extract_text_config(
            candidate,
            bytes,
            "linux.profile_script",
            "profileScript",
            outcome,
        ),
        LinuxArtifactRouteKind::AptHistory => {
            packages::extract_apt_history(candidate, bytes, outcome, log_time)
        }
        LinuxArtifactRouteKind::DpkgLog => {
            packages::extract_dpkg_log(candidate, bytes, outcome, log_time)
        }
        LinuxArtifactRouteKind::RpmLog => {
            packages::extract_rpm_package_log(candidate, bytes, outcome, log_time)
        }
        LinuxArtifactRouteKind::Cron => cron::extract(candidate, bytes, outcome),
        LinuxArtifactRouteKind::AuthLog => extract_auth_log(candidate, bytes, outcome, log_time),
        LinuxArtifactRouteKind::TextLog => {
            text_log::extract(candidate, bytes, "linux.text_log", "log", outcome, log_time)
        }
        LinuxArtifactRouteKind::PveLog => {
            text_log::extract(candidate, bytes, "linux.pve_log", "pve", outcome, log_time)
        }
        LinuxArtifactRouteKind::Unsupported => warn_unsupported_candidate(candidate, outcome),
    }
}

fn extract_text_config(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    parser: &str,
    config_kind: &str,
    outcome: &mut ExtractionOutcome,
) {
    system_config::extract_text_config(candidate, bytes, parser, config_kind, outcome);
}

fn extract_auth_log(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
    log_time: &LinuxLogTimeContext,
) {
    // Dual channel: sudo lines are extracted as structured LinuxSudoEvent
    // records; every other line (sshd, pam, cron sessions, ...) still flows
    // through the text-log fallback. The filter keeps sudo lines out of the
    // fallback so no line is emitted twice.
    sudo::extract(candidate, bytes, outcome, log_time);
    text_log::extract_with_filter(
        candidate,
        bytes,
        "linux.auth_log",
        "auth",
        &sudo::is_sudo_event_line,
        outcome,
        log_time,
    );
}

fn warn_unsupported_candidate(candidate: &EvidenceCandidate, outcome: &mut ExtractionOutcome) {
    let source_path = if candidate.path.is_empty() {
        "<unknown>"
    } else {
        candidate.path.as_str()
    };
    outcome.warnings.push(format!(
        "{source_path} is a Linux artifact candidate, but this first-pass parser does not yet extract structured records for it"
    ));
}

pub(super) fn unsupported_linux_candidate_outcome(
    candidate: &EvidenceCandidate,
) -> ExtractionOutcome {
    let mut outcome = ExtractionOutcome::default();
    warn_unsupported_candidate(candidate, &mut outcome);
    outcome
}
