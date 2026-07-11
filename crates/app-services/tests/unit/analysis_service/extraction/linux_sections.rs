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

#[test]
fn linux_route_read_limits_are_path_aware_and_gzip_stable() {
    let large = 128 * 1024 * 1024;
    let text = 16 * 1024 * 1024;
    let small = 4 * 1024 * 1024;

    for path in [
        "/var/log/journal/system.journal",
        "/var/log/wtmp",
        "/var/log/wtmp.gz",
    ] {
        assert_eq!(linux_artifact_route(path).read_limit, large, "{path}");
    }
    for path in [
        "/var/log/auth.log",
        "/var/log/auth.log.1.gz",
        "/var/log/nginx/access.log",
        "/var/log/dpkg.log.1.gz",
    ] {
        assert_eq!(linux_artifact_route(path).read_limit, text, "{path}");
    }
    for path in [
        "/etc/os-release",
        "/etc/nginx/nginx.conf",
        "/etc/mysql/mysql.conf.d/mysqld.cnf",
    ] {
        assert_eq!(linux_artifact_route(path).read_limit, small, "{path}");
    }
}

#[test]
fn linux_route_exposes_support_and_section_from_one_descriptor() {
    let generic_log = linux_artifact_route("/var/log/syslog.1.gz");
    assert_eq!(generic_log.kind, LinuxArtifactRouteKind::TextLog);
    assert_eq!(generic_log.section, LinuxArtifactSection::Journal);
    assert_eq!(generic_log.support, LinuxCandidateSupport::TextFallback);

    let unparsed_login = linux_artifact_route("/var/log/lastlog");
    assert_eq!(unparsed_login.kind, LinuxArtifactRouteKind::Unsupported);
    assert_eq!(unparsed_login.section, LinuxArtifactSection::Login);
    assert_eq!(unparsed_login.support, LinuxCandidateSupport::Unsupported);
}
