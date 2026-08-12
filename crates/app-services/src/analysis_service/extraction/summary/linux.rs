mod mapping;
mod system_info;

use self::mapping::{LinuxCounts, LinuxEntries};
use self::system_info::load_linux_system_info;
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::extraction::observability::{
    linux_summary_observability, linux_summary_status,
};
use chrono::Utc;
use rusqlite::Connection;
use transport::dto::LinuxArtifactSummaryDto;

/// Get Linux artifact summary (systemd journal, text-log fallback lines,
/// wtmp/btmp logins, bash history, apt/dpkg/yum/dnf package events, cron jobs,
/// sudo/auth events, and system config records).
///
/// This is a case-wide aggregate, matching the existing Registry/Browser/EVTX
/// summary functions.
pub fn get_linux_artifact_summary(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<LinuxArtifactSummaryDto, AnalysisServiceError> {
    let counts = LinuxCounts::load(conn)?;
    let entries = LinuxEntries::load(conn, offset, limit)?;
    let system_info = load_linux_system_info(conn)?;
    let total_count = counts.total();
    let observability = linux_summary_observability(conn, total_count)?;

    Ok(LinuxArtifactSummaryDto {
        status: linux_summary_status(total_count, observability.candidate_count),
        journal_count: counts.journal,
        text_log_count: counts.text_log,
        login_count: counts.login,
        bash_command_count: counts.bash_command,
        apt_event_count: counts.apt_event,
        cron_job_count: counts.cron_job,
        sudo_event_count: counts.sudo_event,
        system_config_count: counts.system_config,
        web_site_count: counts.web_site,
        web_access_log_count: counts.web_access_log,
        web_error_log_count: counts.web_error_log,
        web_finding_count: counts.web_finding,
        mysql_config_count: counts.mysql_config,
        mysql_log_count: counts.mysql_log,
        mysql_finding_count: counts.mysql_finding,
        total_count,
        truncated: observability.truncated,
        coverage_ratio: observability.coverage_ratio,
        journal_entries: entries.journal,
        login_records: entries.login,
        bash_commands: entries.bash_commands,
        apt_events: entries.apt_events,
        cron_jobs: entries.cron_jobs,
        sudo_events: entries.sudo_events,
        system_configs: entries.system_configs,
        web_sites: entries.web_sites,
        web_access_logs: entries.web_access_logs,
        web_error_logs: entries.web_error_logs,
        web_findings: entries.web_findings,
        mysql_configs: entries.mysql_configs,
        mysql_logs: entries.mysql_logs,
        mysql_findings: entries.mysql_findings,
        system_info,
        generated_at: Utc::now().to_rfc3339(),
        warnings: observability.warnings,
    })
}
