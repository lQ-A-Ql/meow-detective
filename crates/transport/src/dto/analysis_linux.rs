use serde::{Deserialize, Serialize};

use crate::dto::analysis_base::AnalysisParseStatusDto;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxArtifactSummaryDto {
    pub status: AnalysisParseStatusDto,
    pub journal_count: u64,
    pub login_count: u64,
    pub bash_command_count: u64,
    pub apt_event_count: u64,
    pub cron_job_count: u64,
    pub sudo_event_count: u64,
    pub system_config_count: u64,
    pub web_site_count: u64,
    pub web_access_log_count: u64,
    pub web_error_log_count: u64,
    pub web_finding_count: u64,
    pub mysql_config_count: u64,
    pub mysql_log_count: u64,
    pub mysql_finding_count: u64,
    pub total_count: u64,
    pub truncated: bool,
    pub coverage_ratio: f32,
    pub journal_entries: Vec<LinuxJournalEntryDto>,
    pub login_records: Vec<LinuxLoginRecordDto>,
    pub bash_commands: Vec<LinuxBashCommandDto>,
    pub apt_events: Vec<LinuxAptEventDto>,
    pub cron_jobs: Vec<LinuxCronJobDto>,
    pub sudo_events: Vec<LinuxSudoEventDto>,
    pub system_configs: Vec<LinuxSystemConfigDto>,
    pub web_sites: Vec<LinuxWebSiteDto>,
    pub web_access_logs: Vec<LinuxWebAccessLogDto>,
    pub web_error_logs: Vec<LinuxWebErrorLogDto>,
    pub web_findings: Vec<LinuxWebFindingDto>,
    pub mysql_configs: Vec<LinuxMysqlConfigDto>,
    pub mysql_logs: Vec<LinuxMysqlLogEntryDto>,
    pub mysql_findings: Vec<LinuxMysqlFindingDto>,
    pub generated_at: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxJournalEntryDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub systemd_unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syslog_identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxLoginRecordDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub user: String,
    pub terminal: String,
    pub host: String,
    pub pid: i32,
    pub record_type: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logout_time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxBashCommandDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub command: String,
    pub line_number: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxAptEventDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub action: String,
    pub package: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxCronJobDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub schedule: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    pub source_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxSudoEventDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub user: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_user: Option<String>,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal: Option<String>,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxSystemConfigDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub config_kind: String,
    pub line: String,
    pub line_number: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxWebSiteDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub server_kind: String,
    pub site_name: String,
    pub hostnames: Vec<String>,
    pub listen: Vec<String>,
    pub document_roots: Vec<String>,
    pub access_logs: Vec<String>,
    pub error_logs: Vec<String>,
    pub line_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxWebAccessLogDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub client_ip: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    pub method: String,
    pub uri: String,
    pub protocol: String,
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    pub line_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxWebErrorLogDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    pub message: String,
    pub line_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxWebFindingDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub finding_kind: String,
    pub severity: String,
    pub confidence: f32,
    pub evidence: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    pub line_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxMysqlConfigDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    pub key: String,
    pub value: String,
    pub line_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxMysqlLogEntryDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    pub message: String,
    pub line_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxMysqlFindingDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub finding_kind: String,
    pub severity: String,
    pub confidence: f32,
    pub evidence: String,
    pub line_number: u64,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/analysis_linux.rs"]
mod tests;
