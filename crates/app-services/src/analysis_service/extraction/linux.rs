use super::ExtractionOutcome;
use crate::analysis_service::artifact_builders::{base_attrs, make_artifact, make_timeline_event};
use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};
use crate::analysis_service::MAX_ANALYSIS_SOURCE_BYTES;
use chrono::{DateTime, TimeZone, Utc};
use flate2::read::GzDecoder;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Read;

const MAX_TEXT_LOG_EVENTS_PER_SOURCE: usize = 10_000;
const MAX_WEB_ERROR_LOG_EVENTS_PER_SOURCE: usize = 2_000;
const MAX_MYSQL_LOG_EVENTS_PER_SOURCE: usize = 2_000;
const MAX_LINUX_TEXT_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_LINUX_SMALL_SOURCE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LinuxCandidateSupport {
    Structured,
    TextFallback,
    Unsupported,
}

pub fn extract_linux_candidate(candidate: &EvidenceCandidate, bytes: &[u8]) -> ExtractionOutcome {
    let mut outcome = ExtractionOutcome::default();
    let normalized = normalize_evidence_path(&candidate.path);
    let decoded;
    let effective_path = normalized.strip_suffix(".gz").unwrap_or(&normalized);
    let input = if normalized.ends_with(".gz") {
        match decode_gzip(bytes) {
            Ok((data, decoded_truncated)) => {
                decoded = data;
                if decoded_truncated {
                    outcome.warnings.push(format!(
                        "{} gzip decoded output exceeds the 128 MiB analysis cap; decoded content was truncated before parsing",
                        candidate.path
                    ));
                }
                decoded.as_slice()
            }
            Err(err) => {
                outcome
                    .warnings
                    .push(format!("{} gzip decode failed: {}", candidate.path, err));
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

    if is_journal_path(effective_path) {
        extract_journal(candidate, input, &mut outcome);
    } else if is_nginx_config_path(effective_path) {
        extract_nginx_config(candidate, input, &mut outcome);
    } else if is_apache_config_path(effective_path) {
        extract_apache_config(candidate, input, &mut outcome);
    } else if is_web_access_log_path(effective_path) {
        extract_web_access_log(candidate, input, &mut outcome);
    } else if is_web_error_log_path(effective_path) {
        extract_web_error_log(candidate, input, &mut outcome);
    } else if is_web_root_script_path(effective_path) {
        extract_web_root_script(candidate, input, &mut outcome);
    } else if is_mysql_config_path(effective_path) {
        extract_mysql_config(candidate, input, &mut outcome);
    } else if is_mysql_log_path(effective_path) {
        extract_mysql_log(candidate, input, &mut outcome);
    } else if is_wtmp_path(effective_path) {
        extract_wtmp(candidate, input, &mut outcome);
    } else if is_bash_history_path(effective_path) {
        extract_bash_history(candidate, input, &mut outcome);
    } else if is_zsh_history_path(effective_path) {
        extract_zsh_history(candidate, input, &mut outcome);
    } else if is_fish_history_path(effective_path) {
        extract_fish_history(candidate, input, &mut outcome);
    } else if is_plain_shell_history_path(effective_path) {
        extract_plain_shell_history(candidate, input, &mut outcome);
    } else if is_system_config_path(effective_path) {
        extract_system_config(candidate, input, &mut outcome);
    } else if is_pve_config_path(effective_path) {
        extract_pve_config(candidate, input, &mut outcome);
    } else if is_sudoers_path(effective_path) {
        extract_text_config(candidate, input, "linux.sudoers", "sudoers", &mut outcome);
    } else if is_ssh_text_path(effective_path) {
        extract_text_config(
            candidate,
            input,
            "linux.ssh_config",
            "sshConfig",
            &mut outcome,
        );
    } else if is_systemd_unit_path(effective_path) {
        extract_text_config(
            candidate,
            input,
            "linux.systemd_unit",
            "systemdUnit",
            &mut outcome,
        );
    } else if is_init_script_path(effective_path) {
        extract_text_config(
            candidate,
            input,
            "linux.init_script",
            "initScript",
            &mut outcome,
        );
    } else if is_profile_script_path(effective_path) {
        extract_text_config(
            candidate,
            input,
            "linux.profile_script",
            "profileScript",
            &mut outcome,
        );
    } else if is_apt_history_path(effective_path) {
        extract_apt_history(candidate, input, &mut outcome);
    } else if is_dpkg_log_path(effective_path) {
        extract_dpkg_log(candidate, input, &mut outcome);
    } else if is_rpm_package_log_path(effective_path) {
        extract_rpm_package_log(candidate, input, &mut outcome);
    } else if is_cron_path(effective_path) {
        extract_crontab(candidate, input, &mut outcome);
    } else if is_auth_log_path(effective_path) {
        let before = outcome.artifacts.len();
        extract_sudo_log(candidate, input, &mut outcome);
        if outcome.artifacts.len() == before {
            extract_text_log(candidate, input, "linux.auth_log", "auth", &mut outcome);
        }
    } else if is_text_log_path(effective_path) {
        extract_text_log(candidate, input, "linux.text_log", "log", &mut outcome);
    } else if is_pve_log_path(effective_path) {
        extract_text_log(candidate, input, "linux.pve_log", "pve", &mut outcome);
    } else {
        let source_path = if candidate.path.is_empty() {
            "<unknown>"
        } else {
            candidate.path.as_str()
        };
        outcome.warnings.push(format!(
            "{source_path} is a Linux artifact candidate, but this first-pass parser does not yet extract structured records for it"
        ));
    }

    outcome
}

pub(super) fn linux_candidate_read_limit(normalized_path: &str) -> usize {
    let effective_path = normalized_path
        .strip_suffix(".gz")
        .unwrap_or(normalized_path);
    if is_journal_path(effective_path) || is_wtmp_path(effective_path) {
        MAX_ANALYSIS_SOURCE_BYTES
    } else if is_web_access_log_path(effective_path)
        || is_web_error_log_path(effective_path)
        || is_text_log_path(effective_path)
        || is_pve_log_path(effective_path)
        || is_auth_log_path(effective_path)
        || is_apt_history_path(effective_path)
        || is_dpkg_log_path(effective_path)
        || is_rpm_package_log_path(effective_path)
    {
        MAX_LINUX_TEXT_SOURCE_BYTES
    } else {
        MAX_LINUX_SMALL_SOURCE_BYTES
    }
}

pub(super) fn linux_candidate_support(normalized_path: &str) -> LinuxCandidateSupport {
    let effective_path = normalized_path
        .strip_suffix(".gz")
        .unwrap_or(normalized_path);
    if is_journal_path(effective_path)
        || is_wtmp_path(effective_path)
        || is_bash_history_path(effective_path)
        || is_zsh_history_path(effective_path)
        || is_fish_history_path(effective_path)
        || is_plain_shell_history_path(effective_path)
        || is_web_services_path(effective_path)
        || is_mysql_services_path(effective_path)
        || is_system_config_path(effective_path)
        || is_pve_config_path(effective_path)
        || is_apt_history_path(effective_path)
        || is_dpkg_log_path(effective_path)
        || is_rpm_package_log_path(effective_path)
        || is_cron_path(effective_path)
        || is_auth_log_path(effective_path)
        || is_sudoers_path(effective_path)
        || is_ssh_text_path(effective_path)
        || is_systemd_unit_path(effective_path)
        || is_init_script_path(effective_path)
        || is_profile_script_path(effective_path)
    {
        LinuxCandidateSupport::Structured
    } else if is_pve_log_path(effective_path) || is_text_log_path(effective_path) {
        LinuxCandidateSupport::TextFallback
    } else {
        LinuxCandidateSupport::Unsupported
    }
}

fn decode_gzip(bytes: &[u8]) -> Result<(Vec<u8>, bool), std::io::Error> {
    let mut decoder = GzDecoder::new(bytes);
    let mut decoded = Vec::new();
    decoder
        .by_ref()
        .take(MAX_ANALYSIS_SOURCE_BYTES as u64 + 1)
        .read_to_end(&mut decoded)?;
    let truncated = decoded.len() > MAX_ANALYSIS_SOURCE_BYTES;
    if truncated {
        decoded.truncate(MAX_ANALYSIS_SOURCE_BYTES);
    }
    Ok((decoded, truncated))
}

fn is_journal_path(normalized: &str) -> bool {
    normalized.ends_with(".journal")
        || normalized.ends_with(".journal~")
        || normalized.contains("/var/log/journal/")
        || normalized.contains("/run/log/journal/")
}

pub(super) fn is_wtmp_path(normalized: &str) -> bool {
    normalized.ends_with("/var/log/wtmp")
        || normalized.contains("/var/log/wtmp.")
        || normalized.ends_with("/var/log/btmp")
        || normalized.contains("/var/log/btmp.")
}

pub(super) fn is_login_binary_candidate_path(normalized: &str) -> bool {
    is_wtmp_path(normalized)
        || normalized.ends_with("/var/log/lastlog")
        || normalized.ends_with("/var/log/faillog")
}

pub(super) fn is_bash_history_path(normalized: &str) -> bool {
    normalized.ends_with(".bash_history")
}

pub(super) fn is_zsh_history_path(normalized: &str) -> bool {
    normalized.ends_with(".zsh_history")
}

pub(super) fn is_fish_history_path(normalized: &str) -> bool {
    normalized.ends_with(".fish_history") || normalized.contains("/.local/share/fish/fish_history")
}

pub(super) fn is_plain_shell_history_path(normalized: &str) -> bool {
    normalized.ends_with(".python_history")
}

pub(super) fn is_system_config_path(normalized: &str) -> bool {
    normalized.ends_with("/etc/os-release")
        || normalized.ends_with("/usr/lib/os-release")
        || normalized.ends_with("/etc/passwd")
        || normalized.ends_with("/etc/group")
        || normalized.ends_with("/etc/hostname")
        || normalized.ends_with("/etc/hosts")
        || normalized.ends_with("/etc/fstab")
        || normalized.ends_with("/etc/resolv.conf")
        || normalized.ends_with("/etc/machine-id")
}

pub(super) fn is_pve_config_path(normalized: &str) -> bool {
    normalized.ends_with("/etc/pve/storage.cfg")
        || (normalized.contains("/etc/pve/qemu-server/") && normalized.ends_with(".conf"))
        || (normalized.contains("/etc/pve/lxc/") && normalized.ends_with(".conf"))
        || normalized.ends_with("/etc/pve/corosync.conf")
        || normalized.ends_with("/etc/corosync/corosync.conf")
}

pub(super) fn is_apt_history_path(normalized: &str) -> bool {
    normalized.contains("/var/log/apt/history.log")
}

pub(super) fn is_dpkg_log_path(normalized: &str) -> bool {
    normalized.contains("/var/log/dpkg.log")
}

pub(super) fn is_rpm_package_log_path(normalized: &str) -> bool {
    normalized.ends_with("/var/log/yum.log")
        || normalized.contains("/var/log/yum.log.")
        || normalized.ends_with("/var/log/dnf.log")
        || normalized.contains("/var/log/dnf.log.")
        || normalized.ends_with("/var/log/dnf.rpm.log")
        || normalized.contains("/var/log/dnf.rpm.log.")
}

pub(super) fn is_cron_path(normalized: &str) -> bool {
    normalized.ends_with("/etc/crontab")
        || normalized.contains("/etc/cron.d/")
        || normalized.contains("/etc/cron.daily/")
        || normalized.contains("/etc/cron.hourly/")
        || normalized.contains("/etc/cron.monthly/")
        || normalized.contains("/etc/cron.weekly/")
        || normalized.contains("/var/spool/cron/")
}

pub(super) fn is_auth_log_path(normalized: &str) -> bool {
    normalized.ends_with("/var/log/auth.log")
        || normalized.contains("/var/log/auth.log.")
        || normalized.ends_with("/var/log/secure")
        || normalized.contains("/var/log/secure.")
}

pub(super) fn is_sudoers_path(normalized: &str) -> bool {
    normalized.ends_with("/etc/sudoers") || normalized.contains("/etc/sudoers.d/")
}

fn is_text_log_path(normalized: &str) -> bool {
    normalized.ends_with("/var/log/audit/audit.log")
        || normalized.contains("/var/log/audit/audit.log.")
        || normalized.ends_with("/var/log/syslog")
        || normalized.contains("/var/log/syslog.")
        || normalized.ends_with("/var/log/messages")
        || normalized.contains("/var/log/messages.")
        || normalized.ends_with("/var/log/kern.log")
        || normalized.contains("/var/log/kern.log.")
        || normalized.ends_with("/var/log/cloud-init.log")
        || normalized.contains("/var/log/cloud-init.log.")
}

fn is_pve_log_path(normalized: &str) -> bool {
    normalized.ends_with("/var/log/pveproxy/access.log")
        || normalized.contains("/var/log/pveproxy/access.log.")
        || normalized.ends_with("/var/log/pvedaemon.log")
        || normalized.contains("/var/log/pvedaemon.log.")
        || normalized.contains("/var/log/pve/tasks/")
}

pub(super) fn is_ssh_text_path(normalized: &str) -> bool {
    normalized.contains("/.ssh/authorized_keys")
        || normalized.contains("/.ssh/known_hosts")
        || normalized.ends_with("/etc/ssh/ssh_config")
        || normalized.ends_with("/etc/ssh/sshd_config")
        || normalized.contains("/etc/ssh/ssh_config.d/")
        || normalized.contains("/etc/ssh/sshd_config.d/")
}

pub(super) fn is_ssh_candidate_path(normalized: &str) -> bool {
    is_ssh_text_path(normalized) || normalized.contains("/etc/ssh/")
}

pub(super) fn is_systemd_unit_path(normalized: &str) -> bool {
    normalized.contains("/etc/systemd/system/")
        || normalized.contains("/lib/systemd/system/")
        || normalized.contains("/usr/lib/systemd/system/")
}

pub(super) fn is_init_script_path(normalized: &str) -> bool {
    normalized.contains("/etc/init.d/") || normalized.ends_with("/etc/rc.local")
}

pub(super) fn is_profile_script_path(normalized: &str) -> bool {
    normalized.contains("/etc/profile.d/")
}

pub(super) fn is_web_services_path(normalized: &str) -> bool {
    is_nginx_config_path(normalized)
        || is_apache_config_path(normalized)
        || is_web_access_log_path(normalized)
        || is_web_error_log_path(normalized)
        || is_web_root_script_path(normalized)
}

pub(super) fn is_mysql_services_path(normalized: &str) -> bool {
    is_mysql_config_path(normalized) || is_mysql_log_path(normalized)
}

pub(super) fn is_mysql_config_path(normalized: &str) -> bool {
    normalized.ends_with("/etc/my.cnf")
        || normalized.ends_with("/etc/mysql/my.cnf")
        || (normalized.contains("/etc/mysql/mysql.conf.d/") && normalized.ends_with(".cnf"))
        || (normalized.contains("/etc/mysql/mariadb.conf.d/") && normalized.ends_with(".cnf"))
        || (normalized.contains("/etc/mysql/conf.d/") && normalized.ends_with(".cnf"))
        || (normalized.contains("/etc/my.cnf.d/") && normalized.ends_with(".cnf"))
}

pub(super) fn is_mysql_log_path(normalized: &str) -> bool {
    normalized.ends_with("/var/log/mysql/error.log")
        || normalized.contains("/var/log/mysql/error.log.")
        || normalized.ends_with("/var/log/mysql/mysql.log")
        || normalized.contains("/var/log/mysql/mysql.log.")
        || normalized.ends_with("/var/log/mysql/mysql-slow.log")
        || normalized.contains("/var/log/mysql/mysql-slow.log.")
        || normalized.ends_with("/var/log/mysql/slow.log")
        || normalized.contains("/var/log/mysql/slow.log.")
        || normalized.ends_with("/var/log/mariadb/mariadb.log")
        || normalized.contains("/var/log/mariadb/mariadb.log.")
        || normalized.ends_with("/var/log/mysqld.log")
        || normalized.contains("/var/log/mysqld.log.")
}

pub(super) fn is_nginx_config_path(normalized: &str) -> bool {
    normalized.ends_with("/etc/nginx/nginx.conf")
        || (normalized.contains("/etc/nginx/conf.d/") && normalized.ends_with(".conf"))
        || normalized.contains("/etc/nginx/sites-enabled/")
}

pub(super) fn is_apache_config_path(normalized: &str) -> bool {
    normalized.ends_with("/etc/apache2/apache2.conf")
        || normalized.contains("/etc/apache2/sites-enabled/")
        || normalized.ends_with("/etc/httpd/conf/httpd.conf")
        || (normalized.contains("/etc/httpd/conf.d/") && normalized.ends_with(".conf"))
}

pub(super) fn is_web_access_log_path(normalized: &str) -> bool {
    normalized.ends_with("/var/log/nginx/access.log")
        || normalized.contains("/var/log/nginx/access.log.")
        || normalized.ends_with("/var/log/apache2/access.log")
        || normalized.contains("/var/log/apache2/access.log.")
        || normalized.ends_with("/var/log/httpd/access_log")
        || normalized.contains("/var/log/httpd/access_log.")
}

pub(super) fn is_web_error_log_path(normalized: &str) -> bool {
    normalized.ends_with("/var/log/nginx/error.log")
        || normalized.contains("/var/log/nginx/error.log.")
        || normalized.ends_with("/var/log/apache2/error.log")
        || normalized.contains("/var/log/apache2/error.log.")
        || normalized.ends_with("/var/log/httpd/error_log")
        || normalized.contains("/var/log/httpd/error_log.")
}

pub(super) fn is_web_root_script_path(normalized: &str) -> bool {
    (normalized.contains("/var/www/") || normalized.contains("/usr/share/nginx/html/"))
        && [".php", ".phtml", ".jsp", ".jspx", ".asp", ".aspx"]
            .iter()
            .any(|suffix| normalized.ends_with(suffix))
}

fn extract_nginx_config(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_nginx_config(&text) {
        Ok(sites) => emit_web_sites(candidate, sites, "linux.nginx_config", outcome),
        Err(err) => outcome.warnings.push(format!(
            "{} nginx config parse failed: {}",
            candidate.path, err
        )),
    }
}

fn extract_apache_config(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_apache_config(&text) {
        Ok(sites) => emit_web_sites(candidate, sites, "linux.apache_config", outcome),
        Err(err) => outcome.warnings.push(format!(
            "{} apache config parse failed: {}",
            candidate.path, err
        )),
    }
}

fn emit_web_sites(
    candidate: &EvidenceCandidate,
    sites: Vec<artifacts_linux::WebSite>,
    parser: &str,
    outcome: &mut ExtractionOutcome,
) {
    if sites.is_empty() {
        outcome.warnings.push(format!(
            "{} contains no auditable nginx/apache site records",
            candidate.path
        ));
    }
    for site in sites {
        let mut attrs = base_attrs(candidate);
        attrs.insert(
            "serverKind".to_string(),
            Value::String(site.server_kind.clone()),
        );
        attrs.insert(
            "siteName".to_string(),
            Value::String(site.site_name.clone()),
        );
        attrs.insert(
            "lineNumber".to_string(),
            Value::Number(site.line_number.into()),
        );
        insert_string_array(&mut attrs, "hostnames", &site.hostnames);
        insert_string_array(&mut attrs, "listen", &site.listen);
        insert_string_array(&mut attrs, "documentRoots", &site.document_roots);
        insert_string_array(&mut attrs, "accessLogs", &site.access_logs);
        insert_string_array(&mut attrs, "errorLogs", &site.error_logs);

        let display_name = site
            .hostnames
            .first()
            .cloned()
            .unwrap_or_else(|| site.site_name.clone());
        outcome.artifacts.push(make_artifact(
            "LinuxWebSite",
            format!("{} site: {}", site.server_kind, display_name),
            format!(
                "listen={}, roots={}",
                site.listen.join(","),
                site.document_roots.join(",")
            ),
            candidate,
            parser,
            attrs,
        ));
    }
}

fn extract_web_access_log(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_web_access_log(&text) {
        Ok(entries) => {
            if entries.len() > MAX_TEXT_LOG_EVENTS_PER_SOURCE {
                outcome.warnings.push(format!(
                    "{} web access log emitted first {} records only",
                    candidate.path, MAX_TEXT_LOG_EVENTS_PER_SOURCE
                ));
            }
            let findings = artifacts_linux::detect_web_findings(&entries);
            for entry in entries.into_iter().take(MAX_TEXT_LOG_EVENTS_PER_SOURCE) {
                emit_web_access_log_entry(candidate, entry, outcome);
            }
            for finding in findings {
                emit_web_finding(candidate, finding, "linux.web_access_log", outcome);
            }
        }
        Err(err) => outcome.warnings.push(format!(
            "{} web access log parse failed: {}",
            candidate.path, err
        )),
    }
}

fn emit_web_access_log_entry(
    candidate: &EvidenceCandidate,
    entry: artifacts_linux::WebAccessLogEntry,
    outcome: &mut ExtractionOutcome,
) {
    let mut attrs = base_attrs(candidate);
    attrs.insert(
        "clientIp".to_string(),
        Value::String(entry.client_ip.clone()),
    );
    attrs.insert("method".to_string(), Value::String(entry.method.clone()));
    attrs.insert("uri".to_string(), Value::String(entry.uri.clone()));
    attrs.insert(
        "protocol".to_string(),
        Value::String(entry.protocol.clone()),
    );
    attrs.insert("status".to_string(), Value::Number(entry.status.into()));
    attrs.insert(
        "lineNumber".to_string(),
        Value::Number(entry.line_number.into()),
    );
    if let Some(response_bytes) = entry.response_bytes {
        attrs.insert(
            "responseBytes".to_string(),
            Value::Number(response_bytes.into()),
        );
    }
    insert_opt(&mut attrs, "referer", entry.referer.clone());
    insert_opt(&mut attrs, "userAgent", entry.user_agent.clone());
    if let Some(ts) = entry.timestamp {
        attrs.insert("timestamp".to_string(), Value::String(ts.to_rfc3339()));
    }

    outcome.artifacts.push(make_artifact(
        "LinuxWebAccessLog",
        format!("Web access {} {} {}", entry.method, entry.status, entry.uri),
        format!("{} {} {}", entry.client_ip, entry.method, entry.uri),
        candidate,
        "linux.web_access_log",
        attrs,
    ));
}

fn extract_web_error_log(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_web_error_log(&text) {
        Ok(entries) => {
            if entries.len() > MAX_WEB_ERROR_LOG_EVENTS_PER_SOURCE {
                outcome.warnings.push(format!(
                    "{} web error log emitted first {} records only",
                    candidate.path, MAX_WEB_ERROR_LOG_EVENTS_PER_SOURCE
                ));
            }
            for entry in entries
                .into_iter()
                .take(MAX_WEB_ERROR_LOG_EVENTS_PER_SOURCE)
            {
                let mut attrs = base_attrs(candidate);
                attrs.insert("message".to_string(), Value::String(entry.message.clone()));
                attrs.insert(
                    "lineNumber".to_string(),
                    Value::Number(entry.line_number.into()),
                );
                insert_opt(&mut attrs, "timestamp", entry.timestamp.clone());
                insert_opt(&mut attrs, "severity", entry.severity.clone());
                outcome.artifacts.push(make_artifact(
                    "LinuxWebErrorLog",
                    format!("Web error: {}", truncate(&entry.message, 80)),
                    entry.message,
                    candidate,
                    "linux.web_error_log",
                    attrs,
                ));
            }
        }
        Err(err) => outcome.warnings.push(format!(
            "{} web error log parse failed: {}",
            candidate.path, err
        )),
    }
}

fn extract_web_root_script(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    for finding in artifacts_linux::detect_web_shell(&text, 1) {
        emit_web_finding(candidate, finding, "linux.web_root_script", outcome);
    }
}

fn extract_mysql_config(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_mysql_config(&text) {
        Ok(entries) => {
            let findings = artifacts_linux::detect_mysql_config_findings(&entries);
            if entries.is_empty() {
                outcome.warnings.push(format!(
                    "{} contains no auditable MySQL/MariaDB config entries",
                    candidate.path
                ));
            }
            for entry in entries {
                emit_mysql_config_entry(candidate, entry, outcome);
            }
            for finding in findings {
                emit_mysql_finding(candidate, finding, "linux.mysql_config", outcome);
            }
        }
        Err(err) => outcome.warnings.push(format!(
            "{} MySQL config parse failed: {}",
            candidate.path, err
        )),
    }
}

fn emit_mysql_config_entry(
    candidate: &EvidenceCandidate,
    entry: artifacts_linux::MysqlConfigEntry,
    outcome: &mut ExtractionOutcome,
) {
    let mut attrs = base_attrs(candidate);
    insert_opt(&mut attrs, "section", entry.section.clone());
    attrs.insert("key".to_string(), Value::String(entry.key.clone()));
    attrs.insert("value".to_string(), Value::String(entry.value.clone()));
    attrs.insert(
        "lineNumber".to_string(),
        Value::Number(entry.line_number.into()),
    );
    outcome.artifacts.push(make_artifact(
        "LinuxMysqlConfig",
        format!("MySQL config {}", entry.key),
        format!("{}={}", entry.key, entry.value),
        candidate,
        "linux.mysql_config",
        attrs,
    ));
}

fn extract_mysql_log(candidate: &EvidenceCandidate, bytes: &[u8], outcome: &mut ExtractionOutcome) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_mysql_log(&text) {
        Ok(entries) => {
            if entries.len() > MAX_MYSQL_LOG_EVENTS_PER_SOURCE {
                outcome.warnings.push(format!(
                    "{} MySQL log emitted first {} records only",
                    candidate.path, MAX_MYSQL_LOG_EVENTS_PER_SOURCE
                ));
            }
            let findings = artifacts_linux::detect_mysql_log_findings(&entries);
            for entry in entries.into_iter().take(MAX_MYSQL_LOG_EVENTS_PER_SOURCE) {
                emit_mysql_log_entry(candidate, entry, outcome);
            }
            for finding in findings {
                emit_mysql_finding(candidate, finding, "linux.mysql_log", outcome);
            }
        }
        Err(err) => outcome.warnings.push(format!(
            "{} MySQL log parse failed: {}",
            candidate.path, err
        )),
    }
}

fn emit_mysql_log_entry(
    candidate: &EvidenceCandidate,
    entry: artifacts_linux::MysqlLogEntry,
    outcome: &mut ExtractionOutcome,
) {
    let mut attrs = base_attrs(candidate);
    attrs.insert("message".to_string(), Value::String(entry.message.clone()));
    attrs.insert(
        "lineNumber".to_string(),
        Value::Number(entry.line_number.into()),
    );
    insert_opt(&mut attrs, "severity", entry.severity.clone());
    insert_opt(&mut attrs, "threadId", entry.thread_id.clone());
    if let Some(ts) = entry.timestamp {
        attrs.insert("timestamp".to_string(), Value::String(ts.to_rfc3339()));
    }
    outcome.artifacts.push(make_artifact(
        "LinuxMysqlLogEntry",
        format!("MySQL log: {}", truncate(&entry.message, 80)),
        entry.message.clone(),
        candidate,
        "linux.mysql_log",
        attrs.clone(),
    ));
    if let Some(ts) = entry.timestamp {
        outcome.timeline_events.push(make_timeline_event(
            &candidate.file_id,
            "linux.mysql_log",
            ts,
            "MySQL service log".to_string(),
            entry.message,
            attrs,
            "linux.mysql_log",
        ));
    }
}

fn emit_mysql_finding(
    candidate: &EvidenceCandidate,
    finding: artifacts_linux::MysqlFinding,
    parser: &str,
    outcome: &mut ExtractionOutcome,
) {
    let mut attrs = base_attrs(candidate);
    attrs.insert(
        "findingKind".to_string(),
        Value::String(finding.finding_kind.clone()),
    );
    attrs.insert(
        "severity".to_string(),
        Value::String(finding.severity.clone()),
    );
    attrs.insert(
        "confidence".to_string(),
        Value::Number(
            serde_json::Number::from_f64(finding.confidence as f64)
                .unwrap_or_else(|| serde_json::Number::from(0)),
        ),
    );
    attrs.insert(
        "evidence".to_string(),
        Value::String(finding.evidence.clone()),
    );
    attrs.insert(
        "lineNumber".to_string(),
        Value::Number(finding.line_number.into()),
    );

    outcome.artifacts.push(make_artifact(
        "LinuxMysqlFinding",
        format!(
            "MySQL finding {}: {}",
            finding.severity,
            truncate(&finding.finding_kind, 80)
        ),
        finding.evidence,
        candidate,
        parser,
        attrs,
    ));
}

fn emit_web_finding(
    candidate: &EvidenceCandidate,
    finding: artifacts_linux::WebFinding,
    parser: &str,
    outcome: &mut ExtractionOutcome,
) {
    let mut attrs = base_attrs(candidate);
    attrs.insert(
        "findingKind".to_string(),
        Value::String(finding.finding_kind.clone()),
    );
    attrs.insert(
        "severity".to_string(),
        Value::String(finding.severity.clone()),
    );
    attrs.insert(
        "confidence".to_string(),
        Value::Number(
            serde_json::Number::from_f64(finding.confidence as f64)
                .unwrap_or_else(|| serde_json::Number::from(0)),
        ),
    );
    attrs.insert(
        "evidence".to_string(),
        Value::String(finding.evidence.clone()),
    );
    attrs.insert(
        "lineNumber".to_string(),
        Value::Number(finding.line_number.into()),
    );
    insert_opt(&mut attrs, "clientIp", finding.client_ip.clone());
    insert_opt(&mut attrs, "uri", finding.uri.clone());
    if let Some(ts) = finding.timestamp {
        attrs.insert("timestamp".to_string(), Value::String(ts.to_rfc3339()));
    }

    outcome.artifacts.push(make_artifact(
        "LinuxWebFinding",
        format!(
            "Web finding {}: {}",
            finding.severity,
            truncate(&finding.finding_kind, 80)
        ),
        finding.evidence.clone(),
        candidate,
        parser,
        attrs.clone(),
    ));

    if let Some(ts) = finding.timestamp {
        outcome.timeline_events.push(make_timeline_event(
            &candidate.file_id,
            parser,
            ts,
            format!("Web finding: {}", finding.finding_kind),
            finding.evidence,
            attrs,
            parser,
        ));
    }
}

fn extract_journal(candidate: &EvidenceCandidate, bytes: &[u8], outcome: &mut ExtractionOutcome) {
    match artifacts_linux::parse_journal(bytes) {
        Ok(entries) => {
            for entry in entries {
                let mut attrs = base_attrs(candidate);
                insert_opt(&mut attrs, "message", entry.message.clone());
                insert_opt(&mut attrs, "executable", entry.executable.clone());
                insert_opt(&mut attrs, "cmdline", entry.cmdline.clone());
                insert_opt(&mut attrs, "systemdUnit", entry.systemd_unit.clone());
                insert_opt(&mut attrs, "hostname", entry.hostname.clone());
                insert_opt(
                    &mut attrs,
                    "syslogIdentifier",
                    entry.syslog_identifier.clone(),
                );
                insert_opt(&mut attrs, "bootId", entry.boot_id.clone());
                insert_opt(&mut attrs, "messageId", entry.message_id.clone());
                if let Some(pid) = entry.pid {
                    attrs.insert("pid".to_string(), Value::Number(pid.into()));
                }
                if let Some(uid) = entry.uid {
                    attrs.insert("uid".to_string(), Value::Number(uid.into()));
                }
                if let Some(priority) = entry.priority {
                    attrs.insert("priority".to_string(), Value::Number(priority.into()));
                }
                if !entry.raw_fields.is_empty() {
                    attrs.insert(
                        "rawFields".to_string(),
                        Value::Object(
                            entry
                                .raw_fields
                                .into_iter()
                                .map(|(k, v)| (k, Value::String(v)))
                                .collect(),
                        ),
                    );
                }

                if let Some(ts) = entry.timestamp {
                    attrs.insert("timestamp".to_string(), Value::String(ts.to_rfc3339()));
                }

                let title = entry
                    .message
                    .as_deref()
                    .map(|m| format!("Journal: {}", truncate(m, 80)))
                    .unwrap_or_else(|| "Journal entry".to_string());
                outcome.artifacts.push(make_artifact(
                    "LinuxJournal",
                    title.clone(),
                    title,
                    candidate,
                    "linux.journal",
                    attrs.clone(),
                ));

                if let Some(ts) = entry.timestamp {
                    outcome.timeline_events.push(make_timeline_event(
                        &candidate.file_id,
                        "linux.journal",
                        ts,
                        "Journal message".to_string(),
                        entry.message.unwrap_or_default(),
                        attrs,
                        "linux.journal",
                    ));
                }
            }
        }
        Err(err) => outcome
            .warnings
            .push(format!("{} journal parse failed: {}", candidate.path, err)),
    }
}

fn extract_wtmp(candidate: &EvidenceCandidate, bytes: &[u8], outcome: &mut ExtractionOutcome) {
    match artifacts_linux::parse_wtmp(bytes) {
        Ok(records) => {
            for record in records {
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
                if let Some(ts) = record.login_time {
                    attrs.insert("loginTime".to_string(), Value::String(ts.to_rfc3339()));
                }
                if let Some(ts) = record.logout_time {
                    attrs.insert("logoutTime".to_string(), Value::String(ts.to_rfc3339()));
                }

                let (event_type, title) = wtmp_event_title(&record);
                outcome.artifacts.push(make_artifact(
                    "LinuxWtmp",
                    title.clone(),
                    title,
                    candidate,
                    "linux.wtmp",
                    attrs.clone(),
                ));

                if let Some(ts) = record.login_time {
                    outcome.timeline_events.push(make_timeline_event(
                        &candidate.file_id,
                        event_type,
                        ts,
                        format!(
                            "{} {}@{} ({})",
                            event_type, record.user, record.host, record.terminal
                        ),
                        format!("PID {} record_type {}", record.pid, record.record_type),
                        attrs.clone(),
                        "linux.wtmp",
                    ));
                }
                if let Some(ts) = record.logout_time {
                    outcome.timeline_events.push(make_timeline_event(
                        &candidate.file_id,
                        "logout",
                        ts,
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
        }
        Err(err) => outcome
            .warnings
            .push(format!("{} wtmp parse failed: {}", candidate.path, err)),
    }
}

fn wtmp_event_title(record: &artifacts_linux::LoginRecord) -> (&'static str, String) {
    match record.record_type {
        2 => (
            "boot",
            format!(
                "Boot: {}",
                record
                    .login_time
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_default()
            ),
        ),
        1 => ("runlevel", format!("Runlevel: {}", record.user)),
        _ => (
            "login",
            format!(
                "Login {}@{} via {} ({})",
                record.user,
                record.host,
                record.terminal,
                record
                    .login_time
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_default()
            ),
        ),
    }
}

fn extract_bash_history(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_bash_history(&text) {
        Ok(commands) => {
            for cmd in commands {
                let mut attrs = base_attrs(candidate);
                attrs.insert("command".to_string(), Value::String(cmd.command.clone()));
                attrs.insert(
                    "lineNumber".to_string(),
                    Value::Number(cmd.line_number.into()),
                );
                if let Some(ts) = cmd.timestamp {
                    attrs.insert("timestamp".to_string(), Value::String(ts.to_rfc3339()));
                }

                outcome.artifacts.push(make_artifact(
                    "LinuxBashCommand",
                    format!("Bash: {}", truncate(&cmd.command, 80)),
                    cmd.command.clone(),
                    candidate,
                    "linux.bash_history",
                    attrs.clone(),
                ));

                if let Some(ts) = cmd.timestamp {
                    outcome.timeline_events.push(make_timeline_event(
                        &candidate.file_id,
                        "linux.bash_command",
                        ts,
                        format!("Bash command: {}", truncate(&cmd.command, 80)),
                        cmd.command,
                        attrs,
                        "linux.bash_history",
                    ));
                }
            }
        }
        Err(err) => outcome.warnings.push(format!(
            "{} bash history parse failed: {}",
            candidate.path, err
        )),
    }
}

fn extract_zsh_history(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    let mut commands = Vec::new();
    for (line_number, line) in (1u64..).zip(text.lines()) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (timestamp, command) = parse_zsh_history_line(trimmed);
        if command.is_empty() {
            continue;
        }
        commands.push(ShellHistoryCommand {
            command,
            timestamp,
            line_number,
            shell: "zsh",
        });
    }
    push_shell_history_commands(candidate, commands, "linux.zsh_history", outcome);
}

fn extract_fish_history(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    let mut commands = Vec::new();
    let mut current_command: Option<(u64, String)> = None;
    let mut current_timestamp: Option<DateTime<Utc>> = None;

    for (line_number, line) in (1u64..).zip(text.lines()) {
        let trimmed = line.trim();
        if let Some(command) = trimmed.strip_prefix("- cmd:") {
            if let Some((previous_line, previous_command)) = current_command.take() {
                commands.push(ShellHistoryCommand {
                    command: previous_command,
                    timestamp: current_timestamp.take(),
                    line_number: previous_line,
                    shell: "fish",
                });
            }
            current_command = Some((line_number, unescape_fish_value(command.trim())));
        } else if let Some(when) = trimmed.strip_prefix("when:") {
            current_timestamp = when
                .trim()
                .parse::<i64>()
                .ok()
                .and_then(|epoch| Utc.timestamp_opt(epoch, 0).single());
        }
    }

    if let Some((line_number, command)) = current_command {
        commands.push(ShellHistoryCommand {
            command,
            timestamp: current_timestamp,
            line_number,
            shell: "fish",
        });
    }

    push_shell_history_commands(candidate, commands, "linux.fish_history", outcome);
}

fn extract_plain_shell_history(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    let commands = text
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let command = line.trim();
            if command.is_empty() || command.starts_with('#') {
                return None;
            }
            Some(ShellHistoryCommand {
                command: command.to_string(),
                timestamp: None,
                line_number: index as u64 + 1,
                shell: "shell",
            })
        })
        .collect::<Vec<_>>();
    push_shell_history_commands(candidate, commands, "linux.shell_history", outcome);
}

fn extract_system_config(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let normalized = normalize_evidence_path(&candidate.path);
    let text = String::from_utf8_lossy(bytes);

    if normalized.ends_with("/etc/os-release") || normalized.ends_with("/usr/lib/os-release") {
        match artifacts_linux::parse_os_release(&text) {
            Ok(info) => {
                let mut attrs = base_attrs(candidate);
                attrs.insert(
                    "configKind".to_string(),
                    Value::String("osRelease".to_string()),
                );
                insert_opt(&mut attrs, "prettyName", info.pretty_name.clone());
                insert_opt(&mut attrs, "osId", info.id.clone());
                insert_opt(&mut attrs, "versionId", info.version_id.clone());
                if !info.fields.is_empty() {
                    attrs.insert(
                        "fields".to_string(),
                        Value::Object(
                            info.fields
                                .into_iter()
                                .map(|(key, value)| (key, Value::String(value)))
                                .collect(),
                        ),
                    );
                }

                let title = info
                    .pretty_name
                    .as_deref()
                    .map(|name| format!("Linux OS: {name}"))
                    .unwrap_or_else(|| "Linux OS release".to_string());
                outcome.artifacts.push(make_artifact(
                    "LinuxSystemConfig",
                    title.clone(),
                    title,
                    candidate,
                    "linux.os_release",
                    attrs,
                ));
            }
            Err(err) => outcome.warnings.push(format!(
                "{} os-release parse failed: {}",
                candidate.path, err
            )),
        }
        return;
    }

    if normalized.ends_with("/etc/passwd") {
        match artifacts_linux::parse_passwd(&text) {
            Ok(accounts) => {
                for account in accounts {
                    let mut attrs = base_attrs(candidate);
                    attrs.insert(
                        "configKind".to_string(),
                        Value::String("passwdAccount".to_string()),
                    );
                    attrs.insert(
                        "username".to_string(),
                        Value::String(account.username.clone()),
                    );
                    attrs.insert("uid".to_string(), Value::Number(account.uid.into()));
                    attrs.insert("gid".to_string(), Value::Number(account.gid.into()));
                    attrs.insert("gecos".to_string(), Value::String(account.gecos.clone()));
                    attrs.insert("home".to_string(), Value::String(account.home.clone()));
                    attrs.insert("shell".to_string(), Value::String(account.shell.clone()));
                    attrs.insert("isUidZero".to_string(), Value::Bool(account.uid == 0));
                    attrs.insert(
                        "hasInteractiveShell".to_string(),
                        Value::Bool(is_interactive_shell(&account.shell)),
                    );

                    outcome.artifacts.push(make_artifact(
                        "LinuxSystemConfig",
                        format!("Linux account: {}", account.username),
                        format!(
                            "uid={} gid={} home={} shell={}",
                            account.uid, account.gid, account.home, account.shell
                        ),
                        candidate,
                        "linux.passwd",
                        attrs,
                    ));
                }
            }
            Err(err) => outcome
                .warnings
                .push(format!("{} passwd parse failed: {}", candidate.path, err)),
        }
        return;
    }

    extract_key_value_or_lines(candidate, &text, "linux.system_config", outcome);
}

fn extract_pve_config(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    if std::str::from_utf8(bytes).is_err() {
        outcome.warnings.push(format!(
            "{} contains non-UTF-8 bytes; invalid sequences were replaced before PVE config extraction",
            candidate.path
        ));
    }

    let normalized = normalize_evidence_path(&candidate.path);
    let config_type = pve_config_type(&normalized);
    let text = String::from_utf8_lossy(bytes);
    let mut emitted = 0usize;
    for (line_number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if emitted >= MAX_TEXT_LOG_EVENTS_PER_SOURCE {
            outcome.warnings.push(format!(
                "{} PVE config emitted first {} records only",
                candidate.path, MAX_TEXT_LOG_EVENTS_PER_SOURCE
            ));
            break;
        }

        let mut attrs = base_attrs(candidate);
        attrs.insert(
            "configKind".to_string(),
            Value::String("pveConfig".to_string()),
        );
        attrs.insert(
            "pveConfigType".to_string(),
            Value::String(config_type.to_string()),
        );
        attrs.insert("line".to_string(), Value::String(trimmed.to_string()));
        attrs.insert(
            "lineNumber".to_string(),
            Value::Number((line_number as u64 + 1).into()),
        );
        if let Some((key, value)) = parse_pve_config_pair(trimmed) {
            attrs.insert("key".to_string(), Value::String(key.to_string()));
            attrs.insert("value".to_string(), Value::String(value.to_string()));
        }

        outcome.artifacts.push(make_artifact(
            "LinuxSystemConfig",
            format!("PVE config: {}", truncate(trimmed, 80)),
            trimmed.to_string(),
            candidate,
            "linux.pve_config",
            attrs,
        ));
        emitted += 1;
    }

    if emitted == 0 {
        outcome.warnings.push(format!(
            "{} PVE config contained no auditable non-comment records",
            candidate.path
        ));
    }
}

fn pve_config_type(normalized: &str) -> &'static str {
    if normalized.ends_with("/etc/pve/storage.cfg") {
        "pveStorageConfig"
    } else if normalized.contains("/etc/pve/qemu-server/") {
        "pveQemuConfig"
    } else if normalized.contains("/etc/pve/lxc/") {
        "pveLxcConfig"
    } else if normalized.ends_with("/etc/pve/corosync.conf")
        || normalized.ends_with("/etc/corosync/corosync.conf")
    {
        "pveCorosyncConfig"
    } else {
        "pveConfig"
    }
}

fn parse_pve_config_pair(line: &str) -> Option<(&str, &str)> {
    if let Some((key, value)) = line.split_once(':') {
        let key = key.trim();
        let value = value.trim();
        if !key.is_empty() && !value.is_empty() {
            return Some((key, value));
        }
    }
    if let Some((key, value)) = line.split_once('=') {
        let key = key.trim();
        let value = value.trim();
        if !key.is_empty() && !value.is_empty() {
            return Some((key, value));
        }
    }

    let mut parts = line.splitn(2, char::is_whitespace);
    let key = parts.next()?.trim();
    let value = parts.next()?.trim();
    if key.is_empty() || value.is_empty() || value == "{" || value == "}" {
        None
    } else {
        Some((key, value))
    }
}

fn extract_key_value_or_lines(
    candidate: &EvidenceCandidate,
    text: &str,
    parser: &str,
    outcome: &mut ExtractionOutcome,
) {
    extract_key_value_or_lines_with_kind(
        candidate,
        text,
        parser,
        "textConfig",
        "Linux config",
        outcome,
    );
}

fn extract_text_config(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    parser: &str,
    config_kind: &str,
    outcome: &mut ExtractionOutcome,
) {
    if std::str::from_utf8(bytes).is_err() {
        outcome.warnings.push(format!(
            "{} contains non-UTF-8 bytes; invalid sequences were replaced before Linux config extraction",
            candidate.path
        ));
    }

    let text = String::from_utf8_lossy(bytes);
    let emitted = extract_key_value_or_lines_with_kind(
        candidate,
        &text,
        parser,
        config_kind,
        "Linux config",
        outcome,
    );
    if emitted == 0 {
        outcome.warnings.push(format!(
            "{} contained no auditable non-comment Linux config records",
            candidate.path
        ));
    }
}

fn extract_key_value_or_lines_with_kind(
    candidate: &EvidenceCandidate,
    text: &str,
    parser: &str,
    config_kind: &str,
    title_prefix: &str,
    outcome: &mut ExtractionOutcome,
) -> usize {
    let mut emitted = 0usize;
    for (line_number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if emitted >= MAX_TEXT_LOG_EVENTS_PER_SOURCE {
            outcome.warnings.push(format!(
                "{} system config emitted first {} records only",
                candidate.path, MAX_TEXT_LOG_EVENTS_PER_SOURCE
            ));
            break;
        }

        let mut attrs = base_attrs(candidate);
        attrs.insert(
            "configKind".to_string(),
            Value::String(config_kind.to_string()),
        );
        attrs.insert("line".to_string(), Value::String(trimmed.to_string()));
        attrs.insert(
            "lineNumber".to_string(),
            Value::Number((line_number as u64 + 1).into()),
        );
        if let Some((key, value)) = parse_config_pair(trimmed) {
            attrs.insert("key".to_string(), Value::String(key.to_string()));
            attrs.insert("value".to_string(), Value::String(value.to_string()));
        }

        outcome.artifacts.push(make_artifact(
            "LinuxSystemConfig",
            format!("{title_prefix}: {}", truncate(trimmed, 80)),
            trimmed.to_string(),
            candidate,
            parser,
            attrs,
        ));
        emitted += 1;
    }
    emitted
}

fn parse_config_pair(line: &str) -> Option<(&str, &str)> {
    if let Some((key, value)) = line.split_once('=') {
        let key = key.trim();
        let value = value.trim();
        if !key.is_empty() && !value.is_empty() {
            return Some((key, value));
        }
    }
    if let Some((key, value)) = line.split_once(':') {
        let key = key.trim();
        let value = value.trim();
        if !key.is_empty() && !value.is_empty() {
            return Some((key, value));
        }
    }
    None
}

fn is_interactive_shell(shell: &str) -> bool {
    let lower = shell.to_ascii_lowercase();
    !(lower.ends_with("/false")
        || lower.ends_with("/nologin")
        || lower.ends_with("/sync")
        || lower == "false"
        || lower == "nologin")
}

struct ShellHistoryCommand {
    command: String,
    timestamp: Option<DateTime<Utc>>,
    line_number: u64,
    shell: &'static str,
}

fn push_shell_history_commands(
    candidate: &EvidenceCandidate,
    commands: Vec<ShellHistoryCommand>,
    parser: &str,
    outcome: &mut ExtractionOutcome,
) {
    for cmd in commands {
        let mut attrs = base_attrs(candidate);
        attrs.insert("command".to_string(), Value::String(cmd.command.clone()));
        attrs.insert(
            "lineNumber".to_string(),
            Value::Number(cmd.line_number.into()),
        );
        attrs.insert("shell".to_string(), Value::String(cmd.shell.to_string()));
        if let Some(ts) = cmd.timestamp {
            attrs.insert("timestamp".to_string(), Value::String(ts.to_rfc3339()));
        }

        outcome.artifacts.push(make_artifact(
            "LinuxBashCommand",
            format!("{}: {}", cmd.shell, truncate(&cmd.command, 80)),
            cmd.command.clone(),
            candidate,
            parser,
            attrs.clone(),
        ));

        if let Some(ts) = cmd.timestamp {
            outcome.timeline_events.push(make_timeline_event(
                &candidate.file_id,
                "linux.shell_command",
                ts,
                format!("{} command: {}", cmd.shell, truncate(&cmd.command, 80)),
                cmd.command,
                attrs,
                parser,
            ));
        }
    }
}

fn parse_zsh_history_line(line: &str) -> (Option<DateTime<Utc>>, String) {
    if let Some(rest) = line.strip_prefix(": ") {
        let mut parts = rest.splitn(2, ';');
        if let (Some(meta), Some(command)) = (parts.next(), parts.next()) {
            let epoch = meta
                .split(':')
                .next()
                .and_then(|raw| raw.trim().parse::<i64>().ok())
                .and_then(|value| Utc.timestamp_opt(value, 0).single());
            return (epoch, command.trim().to_string());
        }
    }
    (None, line.trim().to_string())
}

fn unescape_fish_value(value: &str) -> String {
    value
        .replace("\\n", "\n")
        .replace("\\:", ":")
        .replace("\\\\", "\\")
}

fn extract_apt_history(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_apt_history(&text) {
        Ok(events) => {
            for event in events {
                push_apt_event(candidate, event, outcome);
            }
        }
        Err(err) => outcome.warnings.push(format!(
            "{} APT history parse failed: {}",
            candidate.path, err
        )),
    }
}

fn extract_dpkg_log(candidate: &EvidenceCandidate, bytes: &[u8], outcome: &mut ExtractionOutcome) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_dpkg_log(&text) {
        Ok(events) => {
            for event in events {
                push_apt_event(candidate, event, outcome);
            }
        }
        Err(err) => outcome
            .warnings
            .push(format!("{} dpkg log parse failed: {}", candidate.path, err)),
    }
}

fn extract_rpm_package_log(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_rpm_package_log(&text) {
        Ok(events) => {
            for event in events {
                push_apt_event(candidate, event, outcome);
            }
        }
        Err(err) => outcome.warnings.push(format!(
            "{} rpm package log parse failed: {}",
            candidate.path, err
        )),
    }
}

fn push_apt_event(
    candidate: &EvidenceCandidate,
    event: artifacts_linux::AptEvent,
    outcome: &mut ExtractionOutcome,
) {
    let mut attrs = base_attrs(candidate);
    attrs.insert("action".to_string(), Value::String(event.action.clone()));
    attrs.insert("package".to_string(), Value::String(event.package.clone()));
    attrs.insert("version".to_string(), Value::String(event.version.clone()));
    if let Some(ts) = event.timestamp {
        attrs.insert("timestamp".to_string(), Value::String(ts.to_rfc3339()));
    }

    outcome.artifacts.push(make_artifact(
        "LinuxAptEvent",
        format!("APT/DPKG {} {}", event.action, event.package),
        format!(
            "{} {} {} ({})",
            event.action,
            event.package,
            event.version,
            event.timestamp.map(|t| t.to_rfc3339()).unwrap_or_default()
        ),
        candidate,
        "linux.apt",
        attrs.clone(),
    ));

    if let Some(ts) = event.timestamp {
        outcome.timeline_events.push(make_timeline_event(
            &candidate.file_id,
            "linux.apt",
            ts,
            format!(
                "Package {} {} {}",
                event.action, event.package, event.version
            ),
            format!("Logged by {} parser", candidate.path),
            attrs,
            "linux.apt",
        ));
    }
}

fn extract_crontab(candidate: &EvidenceCandidate, bytes: &[u8], outcome: &mut ExtractionOutcome) {
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
        Err(err) => outcome
            .warnings
            .push(format!("{} crontab parse failed: {}", candidate.path, err)),
    }
}

fn extract_sudo_log(candidate: &EvidenceCandidate, bytes: &[u8], outcome: &mut ExtractionOutcome) {
    let text = String::from_utf8_lossy(bytes);
    match artifacts_linux::parse_auth_log_sudo(&text) {
        Ok(events) => {
            for event in events {
                let mut attrs = base_attrs(candidate);
                attrs.insert("user".to_string(), Value::String(event.user.clone()));
                attrs.insert("command".to_string(), Value::String(event.command.clone()));
                insert_opt(&mut attrs, "targetUser", event.target_user.clone());
                insert_opt(
                    &mut attrs,
                    "workingDirectory",
                    event.working_directory.clone(),
                );
                insert_opt(&mut attrs, "terminal", event.terminal.clone());
                attrs.insert("success".to_string(), Value::Bool(event.success));
                if let Some(ts) = event.timestamp {
                    attrs.insert("timestamp".to_string(), Value::String(ts.to_rfc3339()));
                }

                outcome.artifacts.push(make_artifact(
                    "LinuxSudoEvent",
                    format!(
                        "sudo {} {} (success={})",
                        event.user,
                        truncate(&event.command, 60),
                        event.success
                    ),
                    format!(
                        "User {} invoked sudo as {:?}: {} (success={})",
                        event.user, event.target_user, event.command, event.success
                    ),
                    candidate,
                    "linux.sudo",
                    attrs.clone(),
                ));

                if let Some(ts) = event.timestamp {
                    outcome.timeline_events.push(make_timeline_event(
                        &candidate.file_id,
                        "linux.sudo",
                        ts,
                        format!("sudo {}: {}", event.user, truncate(&event.command, 80)),
                        format!(
                            "target_user={:?}, success={}",
                            event.target_user, event.success
                        ),
                        attrs,
                        "linux.sudo",
                    ));
                }
            }
        }
        Err(err) => outcome
            .warnings
            .push(format!("{} sudo log parse failed: {}", candidate.path, err)),
    }
}

fn extract_text_log(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    parser: &str,
    label: &str,
    outcome: &mut ExtractionOutcome,
) {
    let text = String::from_utf8_lossy(bytes);
    let mut emitted = 0usize;
    for (line_number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if emitted >= MAX_TEXT_LOG_EVENTS_PER_SOURCE {
            outcome.warnings.push(format!(
                "{} text log emitted first {} records only",
                candidate.path, MAX_TEXT_LOG_EVENTS_PER_SOURCE
            ));
            break;
        }

        let parsed = parse_syslog_like_line(trimmed);
        let mut attrs = base_attrs(candidate);
        attrs.insert("message".to_string(), Value::String(parsed.message.clone()));
        attrs.insert(
            "lineNumber".to_string(),
            Value::Number((line_number as u64 + 1).into()),
        );
        attrs.insert("logKind".to_string(), Value::String(label.to_string()));
        insert_opt(&mut attrs, "hostname", parsed.hostname.clone());
        insert_opt(
            &mut attrs,
            "syslogIdentifier",
            parsed.syslog_identifier.clone(),
        );
        if let Some(pid) = parsed.pid {
            attrs.insert("pid".to_string(), Value::Number(pid.into()));
        }
        if let Some(priority) = parsed.priority {
            attrs.insert("priority".to_string(), Value::Number(priority.into()));
        }
        if let Some(ts) = parsed.timestamp {
            attrs.insert("timestamp".to_string(), Value::String(ts.to_rfc3339()));
        }

        outcome.artifacts.push(make_artifact(
            "LinuxJournal",
            format!("{} log: {}", label, truncate(&parsed.message, 80)),
            parsed.message.clone(),
            candidate,
            parser,
            attrs.clone(),
        ));

        if let Some(ts) = parsed.timestamp {
            outcome.timeline_events.push(make_timeline_event(
                &candidate.file_id,
                parser,
                ts,
                format!("{} log: {}", label, truncate(&parsed.message, 80)),
                parsed.message,
                attrs,
                parser,
            ));
        }
        emitted += 1;
    }
}

struct ParsedTextLogLine {
    timestamp: Option<DateTime<Utc>>,
    hostname: Option<String>,
    syslog_identifier: Option<String>,
    pid: Option<u32>,
    priority: Option<u32>,
    message: String,
}

fn parse_syslog_like_line(line: &str) -> ParsedTextLogLine {
    if let Some(parsed) = parse_rfc3339_log_line(line) {
        return parsed;
    }
    if let Some(parsed) = parse_classic_syslog_line(line) {
        return parsed;
    }
    ParsedTextLogLine {
        timestamp: None,
        hostname: None,
        syslog_identifier: None,
        pid: None,
        priority: audit_priority(line),
        message: line.to_string(),
    }
}

fn parse_rfc3339_log_line(line: &str) -> Option<ParsedTextLogLine> {
    let (head, rest) = line.split_once(' ')?;
    let timestamp = DateTime::parse_from_rfc3339(head)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))?;
    let mut parsed = parse_syslog_body(rest);
    parsed.timestamp = Some(timestamp);
    Some(parsed)
}

fn parse_classic_syslog_line(line: &str) -> Option<ParsedTextLogLine> {
    let mut parts = line.splitn(4, ' ');
    let month = parts.next()?;
    let day = parts.next()?;
    let time = parts.next()?;
    let rest = parts.next()?;
    if !is_classic_syslog_timestamp_shape(month, day, time) {
        return None;
    }
    Some(parse_syslog_body(rest))
}

fn parse_syslog_body(body: &str) -> ParsedTextLogLine {
    let (hostname, remainder) = body
        .split_once(' ')
        .map(|(host, rest)| (Some(host.to_string()), rest.trim()))
        .unwrap_or((None, body.trim()));
    let (identifier, pid, message) = parse_identifier_and_message(remainder);
    ParsedTextLogLine {
        timestamp: None,
        hostname,
        syslog_identifier: identifier,
        pid,
        priority: audit_priority(message).or_else(|| syslog_priority(message)),
        message: message.to_string(),
    }
}

fn parse_identifier_and_message(input: &str) -> (Option<String>, Option<u32>, &str) {
    let Some((prefix, message)) = input.split_once(':') else {
        return (None, None, input);
    };
    let prefix = prefix.trim();
    if prefix.is_empty() || prefix.contains(' ') {
        return (None, None, input);
    }
    if let Some((identifier, pid_text)) = prefix.split_once('[') {
        let pid = pid_text.trim_end_matches(']').parse::<u32>().ok();
        return (Some(identifier.to_string()), pid, message.trim());
    }
    (Some(prefix.to_string()), None, message.trim())
}

fn is_classic_syslog_timestamp_shape(month: &str, day: &str, time: &str) -> bool {
    matches!(
        month,
        "Jan"
            | "Feb"
            | "Mar"
            | "Apr"
            | "May"
            | "Jun"
            | "Jul"
            | "Aug"
            | "Sep"
            | "Oct"
            | "Nov"
            | "Dec"
    ) && day.parse::<u32>().is_ok()
        && time.split(':').count() == 3
        && time.split(':').all(|part| part.parse::<u32>().is_ok())
}

fn audit_priority(line: &str) -> Option<u32> {
    if line.contains("type=SYSCALL") || line.contains("type=EXECVE") {
        Some(5)
    } else if line.contains("type=USER_AUTH") || line.contains("type=USER_LOGIN") {
        Some(6)
    } else {
        None
    }
}

fn syslog_priority(message: &str) -> Option<u32> {
    let lower = message.to_ascii_lowercase();
    if lower.contains("error") || lower.contains("fail") || lower.contains("denied") {
        Some(3)
    } else if lower.contains("warn") {
        Some(4)
    } else {
        None
    }
}

fn insert_opt(attrs: &mut BTreeMap<String, Value>, key: &str, value: Option<String>) {
    if let Some(v) = value {
        if !v.is_empty() {
            attrs.insert(key.to_string(), Value::String(v));
        }
    }
}

fn insert_string_array(attrs: &mut BTreeMap<String, Value>, key: &str, values: &[String]) {
    if !values.is_empty() {
        attrs.insert(
            key.to_string(),
            Value::Array(values.iter().cloned().map(Value::String).collect()),
        );
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max_len).collect::<String>())
    }
}
