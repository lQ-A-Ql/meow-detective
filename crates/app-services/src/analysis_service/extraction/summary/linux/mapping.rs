use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::extraction::artifact_query::{
    count_artifacts_by_type, count_artifacts_by_type_and_extractor,
    count_artifacts_by_type_excluding_extractor, query_artifact_rows, AnalysisArtifactRow,
};
use crate::analysis_service::extraction::attr_mapping::{
    bool_attr, i32_attr, optional_string_attr, optional_u32_attr, string_attr, u64_attr,
};
use rusqlite::Connection;
use serde_json::Value;
use std::collections::BTreeMap;
use transport::dto::{
    LinuxAptEventDto, LinuxBashCommandDto, LinuxCronJobDto, LinuxJournalEntryDto,
    LinuxLoginRecordDto, LinuxMysqlConfigDto, LinuxMysqlFindingDto, LinuxMysqlLogEntryDto,
    LinuxSudoEventDto, LinuxSystemConfigDto, LinuxWebAccessLogDto, LinuxWebErrorLogDto,
    LinuxWebFindingDto, LinuxWebSiteDto,
};

pub(super) struct LinuxCounts {
    pub(super) journal: u64,
    /// Text-log fallback records (syslog/messages/auth/audit/pve) share the
    /// `LinuxJournal` artifact type but are counted separately so
    /// `journal_count` reflects only real systemd journal entries.
    pub(super) text_log: u64,
    pub(super) login: u64,
    pub(super) bash_command: u64,
    pub(super) apt_event: u64,
    pub(super) cron_job: u64,
    pub(super) sudo_event: u64,
    pub(super) system_config: u64,
    pub(super) web_site: u64,
    pub(super) web_access_log: u64,
    pub(super) web_error_log: u64,
    pub(super) web_finding: u64,
    pub(super) mysql_config: u64,
    pub(super) mysql_log: u64,
    pub(super) mysql_finding: u64,
}

impl LinuxCounts {
    pub(super) fn load(conn: &Connection) -> Result<Self, AnalysisServiceError> {
        Ok(Self {
            journal: count_artifacts_by_type_and_extractor(conn, "LinuxJournal", "linux.journal")?,
            text_log: count_artifacts_by_type_excluding_extractor(
                conn,
                "LinuxJournal",
                "linux.journal",
            )?,
            login: count_artifacts_by_type(conn, "LinuxWtmp")?,
            bash_command: count_artifacts_by_type(conn, "LinuxBashCommand")?,
            apt_event: count_artifacts_by_type(conn, "LinuxAptEvent")?,
            cron_job: count_artifacts_by_type(conn, "LinuxCronJob")?,
            sudo_event: count_artifacts_by_type(conn, "LinuxSudoEvent")?,
            system_config: count_artifacts_by_type(conn, "LinuxSystemConfig")?,
            web_site: count_artifacts_by_type(conn, "LinuxWebSite")?,
            web_access_log: count_artifacts_by_type(conn, "LinuxWebAccessLog")?,
            web_error_log: count_artifacts_by_type(conn, "LinuxWebErrorLog")?,
            web_finding: count_artifacts_by_type(conn, "LinuxWebFinding")?,
            mysql_config: count_artifacts_by_type(conn, "LinuxMysqlConfig")?,
            mysql_log: count_artifacts_by_type(conn, "LinuxMysqlLogEntry")?,
            mysql_finding: count_artifacts_by_type(conn, "LinuxMysqlFinding")?,
        })
    }

    pub(super) fn total(&self) -> u64 {
        self.journal
            + self.text_log
            + self.login
            + self.bash_command
            + self.apt_event
            + self.cron_job
            + self.sudo_event
            + self.system_config
            + self.web_site
            + self.web_access_log
            + self.web_error_log
            + self.web_finding
            + self.mysql_config
            + self.mysql_log
            + self.mysql_finding
    }
}

pub(super) struct LinuxEntries {
    pub(super) journal: Vec<LinuxJournalEntryDto>,
    pub(super) login: Vec<LinuxLoginRecordDto>,
    pub(super) bash_commands: Vec<LinuxBashCommandDto>,
    pub(super) apt_events: Vec<LinuxAptEventDto>,
    pub(super) cron_jobs: Vec<LinuxCronJobDto>,
    pub(super) sudo_events: Vec<LinuxSudoEventDto>,
    pub(super) system_configs: Vec<LinuxSystemConfigDto>,
    pub(super) web_sites: Vec<LinuxWebSiteDto>,
    pub(super) web_access_logs: Vec<LinuxWebAccessLogDto>,
    pub(super) web_error_logs: Vec<LinuxWebErrorLogDto>,
    pub(super) web_findings: Vec<LinuxWebFindingDto>,
    pub(super) mysql_configs: Vec<LinuxMysqlConfigDto>,
    pub(super) mysql_logs: Vec<LinuxMysqlLogEntryDto>,
    pub(super) mysql_findings: Vec<LinuxMysqlFindingDto>,
}

impl LinuxEntries {
    pub(super) fn load(
        conn: &Connection,
        offset: u64,
        limit: u32,
    ) -> Result<Self, AnalysisServiceError> {
        let journal = map_journal(query(conn, "LinuxJournal", offset, limit)?);
        let login = map_logins(query(conn, "LinuxWtmp", offset, limit)?);
        let bash_commands = map_bash_commands(query(conn, "LinuxBashCommand", offset, limit)?);
        let apt_events = map_apt_events(query(conn, "LinuxAptEvent", offset, limit)?);
        let cron_jobs = map_cron_jobs(query(conn, "LinuxCronJob", offset, limit)?);
        let sudo_events = map_sudo_events(query(conn, "LinuxSudoEvent", offset, limit)?);
        let system_configs = map_system_configs(query(conn, "LinuxSystemConfig", offset, limit)?);
        let web_sites = map_web_sites(query(conn, "LinuxWebSite", offset, limit)?);
        let web_access_logs = map_web_access_logs(query(conn, "LinuxWebAccessLog", offset, limit)?);
        let web_error_logs = map_web_error_logs(query(conn, "LinuxWebErrorLog", offset, limit)?);
        let web_findings = map_web_findings(query(conn, "LinuxWebFinding", offset, limit)?);
        let mysql_configs = map_mysql_configs(query(conn, "LinuxMysqlConfig", offset, limit)?);
        let mysql_logs = map_mysql_logs(query(conn, "LinuxMysqlLogEntry", offset, limit)?);
        let mysql_findings = map_mysql_findings(query(conn, "LinuxMysqlFinding", offset, limit)?);
        Ok(Self {
            journal,
            login,
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
        })
    }
}

fn query(
    conn: &Connection,
    artifact_type: &str,
    offset: u64,
    limit: u32,
) -> Result<Vec<AnalysisArtifactRow>, AnalysisServiceError> {
    query_artifact_rows(conn, &[artifact_type], offset, limit)
}

fn map_journal(rows: Vec<AnalysisArtifactRow>) -> Vec<LinuxJournalEntryDto> {
    rows.into_iter()
        .map(|row| {
            // Text-log fallback rows carry an explicit `logKind` attribute;
            // rows produced by the structured journald extractor default to
            // `journald` so the UI can tell the two channels apart.
            let log_kind = optional_string_attr(&row.attrs, "logKind").or_else(|| {
                (row.extractor_id.as_deref() == Some("linux.journal"))
                    .then(|| "journald".to_string())
            });
            LinuxJournalEntryDto {
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
                log_kind,
            }
        })
        .collect()
}

fn map_logins(rows: Vec<AnalysisArtifactRow>) -> Vec<LinuxLoginRecordDto> {
    rows.into_iter()
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
        .collect()
}

fn map_bash_commands(rows: Vec<AnalysisArtifactRow>) -> Vec<LinuxBashCommandDto> {
    rows.into_iter()
        .map(|row| LinuxBashCommandDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            command: string_attr(&row.attrs, "command"),
            line_number: u64_attr(&row.attrs, "lineNumber"),
            timestamp: optional_string_attr(&row.attrs, "timestamp"),
        })
        .collect()
}

fn map_apt_events(rows: Vec<AnalysisArtifactRow>) -> Vec<LinuxAptEventDto> {
    rows.into_iter()
        .map(|row| LinuxAptEventDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            action: string_attr(&row.attrs, "action"),
            package: string_attr(&row.attrs, "package"),
            version: string_attr(&row.attrs, "version"),
            timestamp: optional_string_attr(&row.attrs, "timestamp"),
        })
        .collect()
}

fn map_cron_jobs(rows: Vec<AnalysisArtifactRow>) -> Vec<LinuxCronJobDto> {
    rows.into_iter()
        .map(|row| LinuxCronJobDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            schedule: string_attr(&row.attrs, "schedule"),
            command: string_attr(&row.attrs, "command"),
            user: optional_string_attr(&row.attrs, "user"),
            source_file: string_attr(&row.attrs, "sourceFile"),
        })
        .collect()
}

fn map_sudo_events(rows: Vec<AnalysisArtifactRow>) -> Vec<LinuxSudoEventDto> {
    rows.into_iter()
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
        .collect()
}

fn map_system_configs(rows: Vec<AnalysisArtifactRow>) -> Vec<LinuxSystemConfigDto> {
    rows.into_iter()
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
        .collect()
}

fn map_web_sites(rows: Vec<AnalysisArtifactRow>) -> Vec<LinuxWebSiteDto> {
    rows.into_iter()
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
        .collect()
}

fn map_web_access_logs(rows: Vec<AnalysisArtifactRow>) -> Vec<LinuxWebAccessLogDto> {
    rows.into_iter()
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
        .collect()
}

fn map_web_error_logs(rows: Vec<AnalysisArtifactRow>) -> Vec<LinuxWebErrorLogDto> {
    rows.into_iter()
        .map(|row| LinuxWebErrorLogDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            timestamp: optional_string_attr(&row.attrs, "timestamp"),
            severity: optional_string_attr(&row.attrs, "severity"),
            message: string_attr(&row.attrs, "message"),
            line_number: u64_attr(&row.attrs, "lineNumber"),
        })
        .collect()
}

fn map_web_findings(rows: Vec<AnalysisArtifactRow>) -> Vec<LinuxWebFindingDto> {
    rows.into_iter()
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
        .collect()
}

fn map_mysql_configs(rows: Vec<AnalysisArtifactRow>) -> Vec<LinuxMysqlConfigDto> {
    rows.into_iter()
        .map(|row| LinuxMysqlConfigDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            section: optional_string_attr(&row.attrs, "section"),
            key: string_attr(&row.attrs, "key"),
            value: string_attr(&row.attrs, "value"),
            line_number: u64_attr(&row.attrs, "lineNumber"),
        })
        .collect()
}

fn map_mysql_logs(rows: Vec<AnalysisArtifactRow>) -> Vec<LinuxMysqlLogEntryDto> {
    rows.into_iter()
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
        .collect()
}

fn map_mysql_findings(rows: Vec<AnalysisArtifactRow>) -> Vec<LinuxMysqlFindingDto> {
    rows.into_iter()
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
        .collect()
}

fn string_vec_attr(attrs: &BTreeMap<String, Value>, key: &str) -> Vec<String> {
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

fn optional_u64_attr(attrs: &BTreeMap<String, Value>, key: &str) -> Option<u64> {
    attrs.get(key).and_then(Value::as_u64)
}

fn f32_attr(attrs: &BTreeMap<String, Value>, key: &str) -> f32 {
    attrs.get(key).and_then(Value::as_f64).unwrap_or_default() as f32
}
