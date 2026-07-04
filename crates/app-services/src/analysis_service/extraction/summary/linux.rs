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
use transport::dto::{
    LinuxAptEventDto, LinuxArtifactSummaryDto, LinuxBashCommandDto, LinuxCronJobDto,
    LinuxJournalEntryDto, LinuxLoginRecordDto, LinuxSudoEventDto,
};

/// Get Linux artifact summary (systemd journal, wtmp/btmp logins, bash history,
/// apt/dpkg package events, cron jobs, sudo/auth events).
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

    let journal_rows = query_artifact_rows(conn, &["LinuxJournal"], offset, limit)?;
    let login_rows = query_artifact_rows(conn, &["LinuxWtmp"], offset, limit)?;
    let bash_rows = query_artifact_rows(conn, &["LinuxBashCommand"], offset, limit)?;
    let apt_rows = query_artifact_rows(conn, &["LinuxAptEvent"], offset, limit)?;
    let cron_rows = query_artifact_rows(conn, &["LinuxCronJob"], offset, limit)?;
    let sudo_rows = query_artifact_rows(conn, &["LinuxSudoEvent"], offset, limit)?;

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

    let total_count = journal_count
        + login_count
        + bash_command_count
        + apt_event_count
        + cron_job_count
        + sudo_event_count
        + system_config_count;
    let observability = linux_summary_observability(conn, total_count)?;

    Ok(LinuxArtifactSummaryDto {
        status: linux_summary_status(total_count, observability.candidate_count),
        journal_count,
        login_count,
        bash_command_count,
        apt_event_count,
        cron_job_count,
        sudo_event_count,
        total_count,
        truncated: observability.truncated,
        coverage_ratio: observability.coverage_ratio,
        journal_entries,
        login_records,
        bash_commands,
        apt_events,
        cron_jobs,
        sudo_events,
        generated_at: Utc::now().to_rfc3339(),
        warnings: observability.warnings,
    })
}
