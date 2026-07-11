use super::linux::{
    is_apache_config_path, is_apt_history_path, is_auth_log_path, is_bash_history_path,
    is_cron_path, is_dpkg_log_path, is_fish_history_path, is_init_script_path, is_journal_path,
    is_login_binary_candidate_path, is_mysql_config_path, is_mysql_log_path, is_nginx_config_path,
    is_plain_shell_history_path, is_profile_script_path, is_pve_config_path, is_pve_log_path,
    is_rpm_package_log_path, is_ssh_candidate_path, is_ssh_text_path, is_sudoers_path,
    is_system_config_path, is_systemd_unit_path, is_text_log_path, is_web_access_log_path,
    is_web_error_log_path, is_web_root_script_path, is_wtmp_path, is_zsh_history_path,
};
use crate::analysis_service::MAX_ANALYSIS_SOURCE_BYTES;

const MAX_LINUX_TEXT_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_LINUX_SMALL_SOURCE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum LinuxArtifactSection {
    Journal,
    Login,
    Commands,
    Packages,
    Cron,
    Sudo,
    SystemConfig,
    WebServices,
    MysqlServices,
}

impl LinuxArtifactSection {
    pub(super) const ALL: [LinuxArtifactSection; 9] = [
        LinuxArtifactSection::Journal,
        LinuxArtifactSection::Login,
        LinuxArtifactSection::Commands,
        LinuxArtifactSection::Packages,
        LinuxArtifactSection::Cron,
        LinuxArtifactSection::Sudo,
        LinuxArtifactSection::SystemConfig,
        LinuxArtifactSection::WebServices,
        LinuxArtifactSection::MysqlServices,
    ];

    pub(super) fn key(self) -> &'static str {
        match self {
            Self::Journal => "LinuxJournal",
            Self::Login => "LinuxLogin",
            Self::Commands => "LinuxCommands",
            Self::Packages => "LinuxPackages",
            Self::Cron => "LinuxCron",
            Self::Sudo => "LinuxSudo",
            Self::SystemConfig => "LinuxSystemConfig",
            Self::WebServices => "LinuxWebServices",
            Self::MysqlServices => "LinuxMysqlServices",
        }
    }

    pub(super) fn from_key(key: &str) -> Option<Self> {
        match key {
            "LinuxJournal" => Some(Self::Journal),
            "LinuxLogin" => Some(Self::Login),
            "LinuxCommands" => Some(Self::Commands),
            "LinuxPackages" => Some(Self::Packages),
            "LinuxCron" => Some(Self::Cron),
            "LinuxSudo" => Some(Self::Sudo),
            "LinuxSystemConfig" => Some(Self::SystemConfig),
            "LinuxWebServices" => Some(Self::WebServices),
            "LinuxMysqlServices" => Some(Self::MysqlServices),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LinuxCandidateSupport {
    Structured,
    TextFallback,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LinuxArtifactRouteKind {
    Journal,
    NginxConfig,
    ApacheConfig,
    WebAccessLog,
    WebErrorLog,
    WebRootScript,
    MysqlConfig,
    MysqlLog,
    Login,
    BashHistory,
    ZshHistory,
    FishHistory,
    PlainShellHistory,
    SystemConfig,
    PveConfig,
    Sudoers,
    SshConfig,
    SystemdUnit,
    InitScript,
    ProfileScript,
    AptHistory,
    DpkgLog,
    RpmLog,
    Cron,
    AuthLog,
    TextLog,
    PveLog,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LinuxArtifactRoute {
    pub(super) kind: LinuxArtifactRouteKind,
    pub(super) section: LinuxArtifactSection,
    pub(super) support: LinuxCandidateSupport,
    pub(super) read_limit: usize,
}

pub(super) fn linux_artifact_route(normalized_path: &str) -> LinuxArtifactRoute {
    let path = normalized_path
        .strip_suffix(".gz")
        .unwrap_or(normalized_path);
    let kind = route_kind(path);
    LinuxArtifactRoute {
        section: route_section(kind, path),
        support: route_support(kind),
        read_limit: route_read_limit(kind),
        kind,
    }
}

pub(super) fn linux_artifact_section(normalized_path: &str) -> LinuxArtifactSection {
    linux_artifact_route(normalized_path).section
}

fn route_kind(path: &str) -> LinuxArtifactRouteKind {
    if is_journal_path(path) {
        LinuxArtifactRouteKind::Journal
    } else if is_nginx_config_path(path) {
        LinuxArtifactRouteKind::NginxConfig
    } else if is_apache_config_path(path) {
        LinuxArtifactRouteKind::ApacheConfig
    } else if is_web_access_log_path(path) {
        LinuxArtifactRouteKind::WebAccessLog
    } else if is_web_error_log_path(path) {
        LinuxArtifactRouteKind::WebErrorLog
    } else if is_web_root_script_path(path) {
        LinuxArtifactRouteKind::WebRootScript
    } else if is_mysql_config_path(path) {
        LinuxArtifactRouteKind::MysqlConfig
    } else if is_mysql_log_path(path) {
        LinuxArtifactRouteKind::MysqlLog
    } else if is_wtmp_path(path) {
        LinuxArtifactRouteKind::Login
    } else if is_bash_history_path(path) {
        LinuxArtifactRouteKind::BashHistory
    } else if is_zsh_history_path(path) {
        LinuxArtifactRouteKind::ZshHistory
    } else if is_fish_history_path(path) {
        LinuxArtifactRouteKind::FishHistory
    } else if is_plain_shell_history_path(path) {
        LinuxArtifactRouteKind::PlainShellHistory
    } else if is_system_config_path(path) {
        LinuxArtifactRouteKind::SystemConfig
    } else if is_pve_config_path(path) {
        LinuxArtifactRouteKind::PveConfig
    } else if is_sudoers_path(path) {
        LinuxArtifactRouteKind::Sudoers
    } else if is_ssh_text_path(path) {
        LinuxArtifactRouteKind::SshConfig
    } else if is_systemd_unit_path(path) {
        LinuxArtifactRouteKind::SystemdUnit
    } else if is_init_script_path(path) {
        LinuxArtifactRouteKind::InitScript
    } else if is_profile_script_path(path) {
        LinuxArtifactRouteKind::ProfileScript
    } else if is_apt_history_path(path) {
        LinuxArtifactRouteKind::AptHistory
    } else if is_dpkg_log_path(path) {
        LinuxArtifactRouteKind::DpkgLog
    } else if is_rpm_package_log_path(path) {
        LinuxArtifactRouteKind::RpmLog
    } else if is_cron_path(path) {
        LinuxArtifactRouteKind::Cron
    } else if is_auth_log_path(path) {
        LinuxArtifactRouteKind::AuthLog
    } else if is_text_log_path(path) {
        LinuxArtifactRouteKind::TextLog
    } else if is_pve_log_path(path) {
        LinuxArtifactRouteKind::PveLog
    } else {
        LinuxArtifactRouteKind::Unsupported
    }
}

fn route_section(kind: LinuxArtifactRouteKind, path: &str) -> LinuxArtifactSection {
    match kind {
        LinuxArtifactRouteKind::NginxConfig
        | LinuxArtifactRouteKind::ApacheConfig
        | LinuxArtifactRouteKind::WebAccessLog
        | LinuxArtifactRouteKind::WebErrorLog
        | LinuxArtifactRouteKind::WebRootScript => LinuxArtifactSection::WebServices,
        LinuxArtifactRouteKind::MysqlConfig | LinuxArtifactRouteKind::MysqlLog => {
            LinuxArtifactSection::MysqlServices
        }
        LinuxArtifactRouteKind::Login => LinuxArtifactSection::Login,
        LinuxArtifactRouteKind::BashHistory
        | LinuxArtifactRouteKind::ZshHistory
        | LinuxArtifactRouteKind::FishHistory
        | LinuxArtifactRouteKind::PlainShellHistory => LinuxArtifactSection::Commands,
        LinuxArtifactRouteKind::AptHistory
        | LinuxArtifactRouteKind::DpkgLog
        | LinuxArtifactRouteKind::RpmLog => LinuxArtifactSection::Packages,
        LinuxArtifactRouteKind::Cron => LinuxArtifactSection::Cron,
        LinuxArtifactRouteKind::AuthLog => LinuxArtifactSection::Sudo,
        LinuxArtifactRouteKind::SystemConfig
        | LinuxArtifactRouteKind::PveConfig
        | LinuxArtifactRouteKind::Sudoers
        | LinuxArtifactRouteKind::SshConfig
        | LinuxArtifactRouteKind::SystemdUnit
        | LinuxArtifactRouteKind::InitScript
        | LinuxArtifactRouteKind::ProfileScript => LinuxArtifactSection::SystemConfig,
        LinuxArtifactRouteKind::Unsupported if is_login_binary_candidate_path(path) => {
            LinuxArtifactSection::Login
        }
        LinuxArtifactRouteKind::Unsupported if is_ssh_candidate_path(path) => {
            LinuxArtifactSection::SystemConfig
        }
        LinuxArtifactRouteKind::Journal
        | LinuxArtifactRouteKind::TextLog
        | LinuxArtifactRouteKind::PveLog
        | LinuxArtifactRouteKind::Unsupported => LinuxArtifactSection::Journal,
    }
}

fn route_support(kind: LinuxArtifactRouteKind) -> LinuxCandidateSupport {
    match kind {
        LinuxArtifactRouteKind::TextLog | LinuxArtifactRouteKind::PveLog => {
            LinuxCandidateSupport::TextFallback
        }
        LinuxArtifactRouteKind::Unsupported => LinuxCandidateSupport::Unsupported,
        _ => LinuxCandidateSupport::Structured,
    }
}

fn route_read_limit(kind: LinuxArtifactRouteKind) -> usize {
    match kind {
        LinuxArtifactRouteKind::Journal | LinuxArtifactRouteKind::Login => {
            MAX_ANALYSIS_SOURCE_BYTES
        }
        LinuxArtifactRouteKind::WebAccessLog
        | LinuxArtifactRouteKind::WebErrorLog
        | LinuxArtifactRouteKind::AptHistory
        | LinuxArtifactRouteKind::DpkgLog
        | LinuxArtifactRouteKind::RpmLog
        | LinuxArtifactRouteKind::AuthLog
        | LinuxArtifactRouteKind::TextLog
        | LinuxArtifactRouteKind::PveLog => MAX_LINUX_TEXT_SOURCE_BYTES,
        _ => MAX_LINUX_SMALL_SOURCE_BYTES,
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/extraction/linux_sections.rs"]
mod tests;
