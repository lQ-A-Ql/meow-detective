//! Route-kind dispatch for Linux candidate extraction.
//!
//! Split out of `linux.rs` to keep that facade under the 200-line guard; the
//! route table and the two composed extractors (text config, auth dual
//! channel) live here.

use super::super::linux_sections::{linux_artifact_route, LinuxArtifactRouteKind};
use super::super::ExtractionOutcome;
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::extraction::linux::timezone::LinuxLogTimeContext;

pub(super) fn dispatch_candidate(
    path: &str,
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
    log_time: &LinuxLogTimeContext,
) {
    match linux_artifact_route(path).kind {
        LinuxArtifactRouteKind::Journal => super::journal::extract(candidate, bytes, outcome),
        LinuxArtifactRouteKind::NginxConfig => {
            super::web::extract_nginx_config(candidate, bytes, outcome)
        }
        LinuxArtifactRouteKind::ApacheConfig => {
            super::web::extract_apache_config(candidate, bytes, outcome)
        }
        LinuxArtifactRouteKind::WebAccessLog => {
            super::web::extract_access_log(candidate, bytes, outcome)
        }
        LinuxArtifactRouteKind::WebErrorLog => {
            super::web::extract_error_log(candidate, bytes, outcome)
        }
        LinuxArtifactRouteKind::WebRootScript => {
            super::web::extract_root_script(candidate, bytes, outcome)
        }
        LinuxArtifactRouteKind::MysqlConfig => {
            super::mysql::extract_config(candidate, bytes, outcome)
        }
        LinuxArtifactRouteKind::MysqlLog => super::mysql::extract_log(candidate, bytes, outcome),
        LinuxArtifactRouteKind::Login => super::login::extract(candidate, bytes, outcome),
        LinuxArtifactRouteKind::Lastlog => super::login::extract_lastlog(candidate, bytes, outcome),
        LinuxArtifactRouteKind::Faillog => super::login::extract_faillog(candidate, bytes, outcome),
        LinuxArtifactRouteKind::BashHistory => {
            super::shell_history::extract_bash(candidate, bytes, outcome)
        }
        LinuxArtifactRouteKind::ZshHistory => {
            super::shell_history::extract_zsh(candidate, bytes, outcome)
        }
        LinuxArtifactRouteKind::FishHistory => {
            super::shell_history::extract_fish(candidate, bytes, outcome)
        }
        LinuxArtifactRouteKind::PlainShellHistory => {
            super::shell_history::extract_plain(candidate, bytes, outcome)
        }
        LinuxArtifactRouteKind::SystemConfig => {
            super::system_config::extract(candidate, bytes, outcome)
        }
        LinuxArtifactRouteKind::PveConfig => super::pve::extract_config(candidate, bytes, outcome),
        LinuxArtifactRouteKind::Sudoers => {
            extract_text_config(candidate, bytes, "linux.sudoers", "sudoers", outcome)
        }
        LinuxArtifactRouteKind::SshConfig => {
            extract_text_config(candidate, bytes, "linux.ssh_config", "sshConfig", outcome)
        }
        LinuxArtifactRouteKind::SystemdUnit => {
            super::system_config::extract_systemd_unit_config(candidate, bytes, outcome)
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
            super::packages::extract_apt_history(candidate, bytes, outcome, log_time)
        }
        LinuxArtifactRouteKind::DpkgLog => {
            super::packages::extract_dpkg_log(candidate, bytes, outcome, log_time)
        }
        LinuxArtifactRouteKind::RpmLog => {
            super::packages::extract_rpm_package_log(candidate, bytes, outcome, log_time)
        }
        LinuxArtifactRouteKind::Cron => super::cron::extract(candidate, bytes, outcome),
        LinuxArtifactRouteKind::AuthLog => extract_auth_log(candidate, bytes, outcome, log_time),
        LinuxArtifactRouteKind::TextLog => {
            super::text_log::extract(candidate, bytes, "linux.text_log", "log", outcome, log_time)
        }
        LinuxArtifactRouteKind::PveLog => {
            super::text_log::extract(candidate, bytes, "linux.pve_log", "pve", outcome, log_time)
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
    super::system_config::extract_text_config(candidate, bytes, parser, config_kind, outcome);
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
    super::sudo::extract(candidate, bytes, outcome, log_time);
    super::text_log::extract_with_filter(
        candidate,
        bytes,
        "linux.auth_log",
        "auth",
        &super::sudo::is_sudo_event_line,
        outcome,
        log_time,
    );
}

pub(super) fn warn_unsupported_candidate(
    candidate: &EvidenceCandidate,
    outcome: &mut ExtractionOutcome,
) {
    let source_path = if candidate.path.is_empty() {
        "<unknown>"
    } else {
        candidate.path.as_str()
    };
    outcome.warnings.push(format!(
        "{source_path} is a Linux artifact candidate, but this first-pass parser does not yet extract structured records for it"
    ));
}
