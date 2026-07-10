use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::extraction::artifact_query::{
    count_artifacts_by_type, query_artifact_rows,
};
use crate::analysis_service::extraction::attr_mapping::{
    bool_attr, i32_attr, optional_string_attr, optional_u32_attr, string_attr, u64_attr,
};
use crate::analysis_service::extraction::observability::{
    linux_summary_observability, linux_summary_status,
};
use chrono::Utc;
use rusqlite::Connection;
use serde_json::Value;
use transport::dto::{
    LinuxAptEventDto, LinuxArtifactSummaryDto, LinuxBashCommandDto, LinuxCronJobDto,
    LinuxJournalEntryDto, LinuxLoginRecordDto, LinuxMysqlConfigDto, LinuxMysqlFindingDto,
    LinuxMysqlLogEntryDto, LinuxSudoEventDto, LinuxSystemConfigDto, LinuxWebAccessLogDto,
    LinuxWebErrorLogDto, LinuxWebFindingDto, LinuxWebSiteDto,
};

/// Get Linux artifact summary (systemd journal, wtmp/btmp logins, bash history,
/// apt/dpkg/yum/dnf package events, cron jobs, sudo/auth events, and system config records).
///
/// This is a case-wide aggregate, matching the existing Registry/Browser/EVTX
/// summary functions; it is not yet scoped to a single data source.
pub fn get_linux_artifact_summary(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<LinuxArtifactSummaryDto, AnalysisServiceError> {
    let journal_count = count_artifacts_by_type(conn, "LinuxJournal")?;
    let login_count = count_artifacts_by_type(conn, "LinuxWtmp")?;
    let bash_command_count = count_artifacts_by_type(conn, "LinuxBashCommand")?;
    let apt_event_count = count_artifacts_by_type(conn, "LinuxAptEvent")?;
    let cron_job_count = count_artifacts_by_type(conn, "LinuxCronJob")?;
    let sudo_event_count = count_artifacts_by_type(conn, "LinuxSudoEvent")?;
    let system_config_count = count_artifacts_by_type(conn, "LinuxSystemConfig")?;
    let web_site_count = count_artifacts_by_type(conn, "LinuxWebSite")?;
    let web_access_log_count = count_artifacts_by_type(conn, "LinuxWebAccessLog")?;
    let web_error_log_count = count_artifacts_by_type(conn, "LinuxWebErrorLog")?;
    let web_finding_count = count_artifacts_by_type(conn, "LinuxWebFinding")?;
    let mysql_config_count = count_artifacts_by_type(conn, "LinuxMysqlConfig")?;
    let mysql_log_count = count_artifacts_by_type(conn, "LinuxMysqlLogEntry")?;
    let mysql_finding_count = count_artifacts_by_type(conn, "LinuxMysqlFinding")?;

    let journal_rows = query_artifact_rows(conn, &["LinuxJournal"], offset, limit)?;
    let login_rows = query_artifact_rows(conn, &["LinuxWtmp"], offset, limit)?;
    let bash_rows = query_artifact_rows(conn, &["LinuxBashCommand"], offset, limit)?;
    let apt_rows = query_artifact_rows(conn, &["LinuxAptEvent"], offset, limit)?;
    let cron_rows = query_artifact_rows(conn, &["LinuxCronJob"], offset, limit)?;
    let sudo_rows = query_artifact_rows(conn, &["LinuxSudoEvent"], offset, limit)?;
    let system_config_rows = query_artifact_rows(conn, &["LinuxSystemConfig"], offset, limit)?;
    let web_site_rows = query_artifact_rows(conn, &["LinuxWebSite"], offset, limit)?;
    let web_access_rows = query_artifact_rows(conn, &["LinuxWebAccessLog"], offset, limit)?;
    let web_error_rows = query_artifact_rows(conn, &["LinuxWebErrorLog"], offset, limit)?;
    let web_finding_rows = query_artifact_rows(conn, &["LinuxWebFinding"], offset, limit)?;
    let mysql_config_rows = query_artifact_rows(conn, &["LinuxMysqlConfig"], offset, limit)?;
    let mysql_log_rows = query_artifact_rows(conn, &["LinuxMysqlLogEntry"], offset, limit)?;
    let mysql_finding_rows = query_artifact_rows(conn, &["LinuxMysqlFinding"], offset, limit)?;

    let journal_entries = journal_rows
        .into_iter()
        .map(|row| LinuxJournalEntryDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            timestamp: optional_string_attr(&row.attrs, "timestamp"),
            message: optional_string_attr(&row.attrs, "message"),
            executable: optional_string_attr(&row.attrs, "executable"),
            systemd_unit: optional_string_attr(&row.attrs, "systemdUnit"),
            hostname: optional_string_attr(&row.attrs, "hostname"),
            syslog_identifier: optional_string_attr(&row.attrs, "syslogIdentifier"),
            pid: optional_u32_attr(&row.attrs, "pid"),
            priority: optional_u32_attr(&row.attrs, "priority"),
        })
        .collect::<Vec<_>>();

    let login_records = login_rows
        .into_iter()
        .map(|row| LinuxLoginRecordDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            user: string_attr(&row.attrs, "user"),
            terminal: string_attr(&row.attrs, "terminal"),
            host: string_attr(&row.attrs, "host"),
            pid: i32_attr(&row.attrs, "pid"),
            record_type: i32_attr(&row.attrs, "recordType"),
            login_time: optional_string_attr(&row.attrs, "loginTime"),
            logout_time: optional_string_attr(&row.attrs, "logoutTime"),
        })
        .collect::<Vec<_>>();

    let bash_commands = bash_rows
        .into_iter()
        .map(|row| LinuxBashCommandDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            command: string_attr(&row.attrs, "command"),
            line_number: u64_attr(&row.attrs, "lineNumber"),
            timestamp: optional_string_attr(&row.attrs, "timestamp"),
        })
        .collect::<Vec<_>>();

    let apt_events = apt_rows
        .into_iter()
        .map(|row| LinuxAptEventDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            action: string_attr(&row.attrs, "action"),
            package: string_attr(&row.attrs, "package"),
            version: string_attr(&row.attrs, "version"),
            timestamp: optional_string_attr(&row.attrs, "timestamp"),
        })
        .collect::<Vec<_>>();

    let cron_jobs = cron_rows
        .into_iter()
        .map(|row| LinuxCronJobDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            schedule: string_attr(&row.attrs, "schedule"),
            command: string_attr(&row.attrs, "command"),
            user: optional_string_attr(&row.attrs, "user"),
            source_file: string_attr(&row.attrs, "sourceFile"),
        })
        .collect::<Vec<_>>();

    let sudo_events = sudo_rows
        .into_iter()
        .map(|row| LinuxSudoEventDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            user: string_attr(&row.attrs, "user"),
            target_user: optional_string_attr(&row.attrs, "targetUser"),
            command: string_attr(&row.attrs, "command"),
            working_directory: optional_string_attr(&row.attrs, "workingDirectory"),
            terminal: optional_string_attr(&row.attrs, "terminal"),
            success: bool_attr(&row.attrs, "success"),
            timestamp: optional_string_attr(&row.attrs, "timestamp"),
        })
        .collect::<Vec<_>>();

    let system_configs = system_config_rows
        .into_iter()
        .map(|row| LinuxSystemConfigDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            config_kind: string_attr(&row.attrs, "configKind"),
            line: string_attr(&row.attrs, "line"),
            line_number: u64_attr(&row.attrs, "lineNumber"),
            key: optional_string_attr(&row.attrs, "key"),
            value: optional_string_attr(&row.attrs, "value"),
            username: optional_string_attr(&row.attrs, "username"),
            uid: optional_u32_attr(&row.attrs, "uid"),
            gid: optional_u32_attr(&row.attrs, "gid"),
            home: optional_string_attr(&row.attrs, "home"),
            shell: optional_string_attr(&row.attrs, "shell"),
        })
        .collect::<Vec<_>>();

    let web_sites = web_site_rows
        .into_iter()
        .map(|row| LinuxWebSiteDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            server_kind: string_attr(&row.attrs, "serverKind"),
            site_name: string_attr(&row.attrs, "siteName"),
            hostnames: string_vec_attr(&row.attrs, "hostnames"),
            listen: string_vec_attr(&row.attrs, "listen"),
            document_roots: string_vec_attr(&row.attrs, "documentRoots"),
            access_logs: string_vec_attr(&row.attrs, "accessLogs"),
            error_logs: string_vec_attr(&row.attrs, "errorLogs"),
            line_number: u64_attr(&row.attrs, "lineNumber"),
        })
        .collect::<Vec<_>>();

    let web_access_logs = web_access_rows
        .into_iter()
        .map(|row| LinuxWebAccessLogDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            client_ip: string_attr(&row.attrs, "clientIp"),
            timestamp: optional_string_attr(&row.attrs, "timestamp"),
            method: string_attr(&row.attrs, "method"),
            uri: string_attr(&row.attrs, "uri"),
            protocol: string_attr(&row.attrs, "protocol"),
            status: optional_u32_attr(&row.attrs, "status").unwrap_or_default() as u16,
            response_bytes: optional_u64_attr(&row.attrs, "responseBytes"),
            referer: optional_string_attr(&row.attrs, "referer"),
            user_agent: optional_string_attr(&row.attrs, "userAgent"),
            line_number: u64_attr(&row.attrs, "lineNumber"),
        })
        .collect::<Vec<_>>();

    let web_error_logs = web_error_rows
        .into_iter()
        .map(|row| LinuxWebErrorLogDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            timestamp: optional_string_attr(&row.attrs, "timestamp"),
            severity: optional_string_attr(&row.attrs, "severity"),
            message: string_attr(&row.attrs, "message"),
            line_number: u64_attr(&row.attrs, "lineNumber"),
        })
        .collect::<Vec<_>>();

    let web_findings = web_finding_rows
        .into_iter()
        .map(|row| LinuxWebFindingDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            finding_kind: string_attr(&row.attrs, "findingKind"),
            severity: string_attr(&row.attrs, "severity"),
            confidence: f32_attr(&row.attrs, "confidence"),
            evidence: string_attr(&row.attrs, "evidence"),
            client_ip: optional_string_attr(&row.attrs, "clientIp"),
            uri: optional_string_attr(&row.attrs, "uri"),
            timestamp: optional_string_attr(&row.attrs, "timestamp"),
            line_number: u64_attr(&row.attrs, "lineNumber"),
        })
        .collect::<Vec<_>>();

    let mysql_configs = mysql_config_rows
        .into_iter()
        .map(|row| LinuxMysqlConfigDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            section: optional_string_attr(&row.attrs, "section"),
            key: string_attr(&row.attrs, "key"),
            value: string_attr(&row.attrs, "value"),
            line_number: u64_attr(&row.attrs, "lineNumber"),
        })
        .collect::<Vec<_>>();

    let mysql_logs = mysql_log_rows
        .into_iter()
        .map(|row| LinuxMysqlLogEntryDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            timestamp: optional_string_attr(&row.attrs, "timestamp"),
            severity: optional_string_attr(&row.attrs, "severity"),
            thread_id: optional_string_attr(&row.attrs, "threadId"),
            message: string_attr(&row.attrs, "message"),
            line_number: u64_attr(&row.attrs, "lineNumber"),
        })
        .collect::<Vec<_>>();

    let mysql_findings = mysql_finding_rows
        .into_iter()
        .map(|row| LinuxMysqlFindingDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            finding_kind: string_attr(&row.attrs, "findingKind"),
            severity: string_attr(&row.attrs, "severity"),
            confidence: f32_attr(&row.attrs, "confidence"),
            evidence: string_attr(&row.attrs, "evidence"),
            line_number: u64_attr(&row.attrs, "lineNumber"),
        })
        .collect::<Vec<_>>();

    let total_count = journal_count
        + login_count
        + bash_command_count
        + apt_event_count
        + cron_job_count
        + sudo_event_count
        + system_config_count
        + web_site_count
        + web_access_log_count
        + web_error_log_count
        + web_finding_count
        + mysql_config_count
        + mysql_log_count
        + mysql_finding_count;
    let observability = linux_summary_observability(conn, total_count)?;

    Ok(LinuxArtifactSummaryDto {
        status: linux_summary_status(total_count, observability.candidate_count),
        journal_count,
        login_count,
        bash_command_count,
        apt_event_count,
        cron_job_count,
        sudo_event_count,
        system_config_count,
        web_site_count,
        web_access_log_count,
        web_error_log_count,
        web_finding_count,
        mysql_config_count,
        mysql_log_count,
        mysql_finding_count,
        total_count,
        truncated: observability.truncated,
        coverage_ratio: observability.coverage_ratio,
        journal_entries,
        login_records,
        bash_commands,
        apt_events,
        cron_jobs,
        sudo_events,
        system_configs,
        web_sites,
        web_access_logs,
        web_error_logs,
        web_findings,
        mysql_configs,
        mysql_logs,
        mysql_findings,
        generated_at: Utc::now().to_rfc3339(),
        warnings: observability.warnings,
    })
}

fn string_vec_attr(attrs: &std::collections::BTreeMap<String, Value>, key: &str) -> Vec<String> {
    attrs
        .get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn optional_u64_attr(attrs: &std::collections::BTreeMap<String, Value>, key: &str) -> Option<u64> {
    attrs.get(key).and_then(Value::as_u64)
}

fn f32_attr(attrs: &std::collections::BTreeMap<String, Value>, key: &str) -> f32 {
    attrs.get(key).and_then(Value::as_f64).unwrap_or_default() as f32
}
