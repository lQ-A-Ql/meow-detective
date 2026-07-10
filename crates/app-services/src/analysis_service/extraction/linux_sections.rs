use super::linux::{
    is_apt_history_path, is_auth_log_path, is_bash_history_path, is_cron_path, is_dpkg_log_path,
    is_fish_history_path, is_init_script_path, is_login_binary_candidate_path,
    is_mysql_services_path, is_plain_shell_history_path, is_profile_script_path,
    is_pve_config_path, is_rpm_package_log_path, is_ssh_candidate_path, is_sudoers_path,
    is_system_config_path, is_systemd_unit_path, is_web_services_path, is_zsh_history_path,
};

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
            LinuxArtifactSection::Journal => "LinuxJournal",
            LinuxArtifactSection::Login => "LinuxLogin",
            LinuxArtifactSection::Commands => "LinuxCommands",
            LinuxArtifactSection::Packages => "LinuxPackages",
            LinuxArtifactSection::Cron => "LinuxCron",
            LinuxArtifactSection::Sudo => "LinuxSudo",
            LinuxArtifactSection::SystemConfig => "LinuxSystemConfig",
            LinuxArtifactSection::WebServices => "LinuxWebServices",
            LinuxArtifactSection::MysqlServices => "LinuxMysqlServices",
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            LinuxArtifactSection::Journal => "Linux 日志",
            LinuxArtifactSection::Login => "Linux 登录记录",
            LinuxArtifactSection::Commands => "Linux 命令历史",
            LinuxArtifactSection::Packages => "Linux 软件包记录",
            LinuxArtifactSection::Cron => "Linux 计划任务",
            LinuxArtifactSection::Sudo => "Linux sudo/auth",
            LinuxArtifactSection::SystemConfig => "Linux 系统配置",
            LinuxArtifactSection::WebServices => "Linux Web 服务",
            LinuxArtifactSection::MysqlServices => "Linux MySQL Services",
        }
    }

    pub(super) fn from_key(key: &str) -> Option<Self> {
        match key {
            "LinuxJournal" => Some(LinuxArtifactSection::Journal),
            "LinuxLogin" => Some(LinuxArtifactSection::Login),
            "LinuxCommands" => Some(LinuxArtifactSection::Commands),
            "LinuxPackages" => Some(LinuxArtifactSection::Packages),
            "LinuxCron" => Some(LinuxArtifactSection::Cron),
            "LinuxSudo" => Some(LinuxArtifactSection::Sudo),
            "LinuxSystemConfig" => Some(LinuxArtifactSection::SystemConfig),
            "LinuxWebServices" => Some(LinuxArtifactSection::WebServices),
            "LinuxMysqlServices" => Some(LinuxArtifactSection::MysqlServices),
            _ => None,
        }
    }
}

pub(super) fn linux_artifact_section(normalized_path: &str) -> LinuxArtifactSection {
    let effective_path = normalized_path
        .strip_suffix(".gz")
        .unwrap_or(normalized_path);
    if is_web_services_path(effective_path) {
        LinuxArtifactSection::WebServices
    } else if is_mysql_services_path(effective_path) {
        LinuxArtifactSection::MysqlServices
    } else if is_login_binary_candidate_path(effective_path) {
        LinuxArtifactSection::Login
    } else if is_bash_history_path(effective_path)
        || is_zsh_history_path(effective_path)
        || is_fish_history_path(effective_path)
        || is_plain_shell_history_path(effective_path)
    {
        LinuxArtifactSection::Commands
    } else if is_apt_history_path(effective_path)
        || is_dpkg_log_path(effective_path)
        || is_rpm_package_log_path(effective_path)
    {
        LinuxArtifactSection::Packages
    } else if is_cron_path(effective_path) {
        LinuxArtifactSection::Cron
    } else if is_auth_log_path(effective_path) {
        LinuxArtifactSection::Sudo
    } else if is_system_config_path(effective_path)
        || is_pve_config_path(effective_path)
        || is_ssh_candidate_path(effective_path)
        || is_sudoers_path(effective_path)
        || is_systemd_unit_path(effective_path)
        || is_init_script_path(effective_path)
        || is_profile_script_path(effective_path)
    {
        LinuxArtifactSection::SystemConfig
    } else {
        LinuxArtifactSection::Journal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_sections_route_known_unparsed_candidates_to_domain_sections() {
        assert_eq!(
            linux_artifact_section("/var/log/lastlog"),
            LinuxArtifactSection::Login
        );
        assert_eq!(
            linux_artifact_section("/etc/ssh/ssh_host_rsa_key"),
            LinuxArtifactSection::SystemConfig
        );
        assert_eq!(
            linux_artifact_section("/etc/sudoers"),
            LinuxArtifactSection::SystemConfig
        );
        assert_eq!(
            linux_artifact_section("/var/log/secure"),
            LinuxArtifactSection::Sudo
        );
        assert_eq!(
            linux_artifact_section("/etc/nginx/nginx.conf"),
            LinuxArtifactSection::WebServices
        );
        assert_eq!(
            linux_artifact_section("/var/log/httpd/access_log"),
            LinuxArtifactSection::WebServices
        );
        assert_eq!(
            linux_artifact_section("/etc/mysql/mysql.conf.d/mysqld.cnf"),
            LinuxArtifactSection::MysqlServices
        );
    }
}
