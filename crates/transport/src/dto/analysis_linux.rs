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
    pub total_count: u64,
    pub journal_entries: Vec<LinuxJournalEntryDto>,
    pub login_records: Vec<LinuxLoginRecordDto>,
    pub bash_commands: Vec<LinuxBashCommandDto>,
    pub apt_events: Vec<LinuxAptEventDto>,
    pub cron_jobs: Vec<LinuxCronJobDto>,
    pub sudo_events: Vec<LinuxSudoEventDto>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_artifact_summary_serializes_camel_case() {
        let dto = LinuxArtifactSummaryDto {
            status: AnalysisParseStatusDto::Parsed,
            journal_count: 1,
            login_count: 2,
            bash_command_count: 3,
            apt_event_count: 4,
            cron_job_count: 5,
            sudo_event_count: 6,
            total_count: 21,
            journal_entries: vec![LinuxJournalEntryDto {
                artifact_id: "artifact-1".to_string(),
                file_id: "file-1".to_string(),
                source_path: "/var/log/journal/x.journal".to_string(),
                timestamp: Some("2026-01-01T00:00:00Z".to_string()),
                message: Some("boot".to_string()),
                executable: None,
                systemd_unit: None,
                hostname: None,
                syslog_identifier: None,
                pid: Some(42),
                priority: None,
            }],
            login_records: Vec::new(),
            bash_commands: Vec::new(),
            apt_events: Vec::new(),
            cron_jobs: Vec::new(),
            sudo_events: Vec::new(),
            generated_at: "2026-01-01T00:00:00Z".to_string(),
            warnings: Vec::new(),
        };

        let value = serde_json::to_value(dto).unwrap();

        assert_eq!(value["journalCount"], 1);
        assert_eq!(value["bashCommandCount"], 3);
        assert_eq!(value["sudoEventCount"], 6);
        assert_eq!(value["totalCount"], 21);
        assert_eq!(value["journalEntries"][0]["artifactId"], "artifact-1");
        assert_eq!(value["journalEntries"][0]["pid"], 42);
        assert!(value["journalEntries"][0].get("executable").is_none());
        assert!(value.get("journal_count").is_none());
    }

    #[test]
    fn linux_sudo_event_omits_optional_fields_when_none() {
        let dto = LinuxSudoEventDto {
            artifact_id: "artifact-2".to_string(),
            file_id: "file-2".to_string(),
            source_path: "/var/log/auth.log".to_string(),
            user: "alice".to_string(),
            target_user: None,
            command: "apt update".to_string(),
            working_directory: None,
            terminal: None,
            success: true,
            timestamp: None,
        };

        let value = serde_json::to_value(dto).unwrap();

        assert_eq!(value["success"], true);
        assert!(value.get("targetUser").is_none());
        assert!(value.get("workingDirectory").is_none());
        assert!(value.get("terminal").is_none());
        assert!(value.get("timestamp").is_none());
    }
}
