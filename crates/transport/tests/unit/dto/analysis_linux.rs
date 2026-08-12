use super::*;

#[test]
fn linux_artifact_summary_serializes_camel_case() {
    let dto = LinuxArtifactSummaryDto {
        status: AnalysisParseStatusDto::Parsed,
        journal_count: 1,
        text_log_count: 15,
        login_count: 2,
        bash_command_count: 3,
        apt_event_count: 4,
        cron_job_count: 5,
        sudo_event_count: 6,
        system_config_count: 7,
        web_site_count: 8,
        web_access_log_count: 9,
        web_error_log_count: 10,
        web_finding_count: 11,
        mysql_config_count: 12,
        mysql_log_count: 13,
        mysql_finding_count: 14,
        total_count: 120,
        truncated: true,
        coverage_ratio: 0.75,
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
            log_kind: Some("journald".to_string()),
        }],
        login_records: Vec::new(),
        bash_commands: Vec::new(),
        apt_events: Vec::new(),
        cron_jobs: Vec::new(),
        sudo_events: Vec::new(),
        system_configs: vec![LinuxSystemConfigDto {
            artifact_id: "artifact-config".to_string(),
            file_id: "file-config".to_string(),
            source_path: "/etc/passwd".to_string(),
            config_kind: "passwdAccount".to_string(),
            line: String::new(),
            line_number: 0,
            key: None,
            value: None,
            username: Some("root".to_string()),
            uid: Some(0),
            gid: Some(0),
            home: Some("/root".to_string()),
            shell: Some("/bin/bash".to_string()),
        }],
        web_sites: vec![LinuxWebSiteDto {
            artifact_id: "artifact-web-site".to_string(),
            file_id: "file-nginx".to_string(),
            source_path: "/etc/nginx/nginx.conf".to_string(),
            server_kind: "nginx".to_string(),
            site_name: "nginx server line 1".to_string(),
            hostnames: vec!["example.com".to_string()],
            listen: vec!["80".to_string()],
            document_roots: vec!["/var/www/html".to_string()],
            access_logs: vec!["/var/log/nginx/access.log".to_string()],
            error_logs: vec!["/var/log/nginx/error.log".to_string()],
            line_number: 1,
        }],
        web_access_logs: vec![LinuxWebAccessLogDto {
            artifact_id: "artifact-web-access".to_string(),
            file_id: "file-access".to_string(),
            source_path: "/var/log/nginx/access.log".to_string(),
            client_ip: "192.0.2.10".to_string(),
            timestamp: Some("2026-01-01T00:00:00Z".to_string()),
            method: "GET".to_string(),
            uri: "/".to_string(),
            protocol: "HTTP/1.1".to_string(),
            status: 200,
            response_bytes: Some(42),
            referer: None,
            user_agent: Some("curl".to_string()),
            line_number: 1,
        }],
        web_error_logs: vec![LinuxWebErrorLogDto {
            artifact_id: "artifact-web-error".to_string(),
            file_id: "file-error".to_string(),
            source_path: "/var/log/nginx/error.log".to_string(),
            timestamp: Some("2026/01/01 00:00:00".to_string()),
            severity: Some("error".to_string()),
            message: "connect failed".to_string(),
            line_number: 1,
        }],
        web_findings: vec![LinuxWebFindingDto {
            artifact_id: "artifact-web-finding".to_string(),
            file_id: "file-access".to_string(),
            source_path: "/var/log/nginx/access.log".to_string(),
            finding_kind: "sqlInjection".to_string(),
            severity: "high".to_string(),
            confidence: 0.9,
            evidence: "GET /?id=1 UNION SELECT".to_string(),
            client_ip: Some("192.0.2.10".to_string()),
            uri: Some("/?id=1".to_string()),
            timestamp: Some("2026-01-01T00:00:00Z".to_string()),
            line_number: 1,
        }],
        mysql_configs: vec![LinuxMysqlConfigDto {
            artifact_id: "artifact-mysql-config".to_string(),
            file_id: "file-mycnf".to_string(),
            source_path: "/etc/mysql/my.cnf".to_string(),
            section: Some("mysqld".to_string()),
            key: "bind-address".to_string(),
            value: "0.0.0.0".to_string(),
            line_number: 2,
        }],
        mysql_logs: vec![LinuxMysqlLogEntryDto {
            artifact_id: "artifact-mysql-log".to_string(),
            file_id: "file-mysql-log".to_string(),
            source_path: "/var/log/mysql/error.log".to_string(),
            timestamp: Some("2026-01-01T00:00:00Z".to_string()),
            severity: Some("warning".to_string()),
            thread_id: Some("8".to_string()),
            message: "Access denied for user".to_string(),
            line_number: 1,
        }],
        mysql_findings: vec![LinuxMysqlFindingDto {
            artifact_id: "artifact-mysql-finding".to_string(),
            file_id: "file-mycnf".to_string(),
            source_path: "/etc/mysql/my.cnf".to_string(),
            finding_kind: "bindAddressAny".to_string(),
            severity: "medium".to_string(),
            confidence: 0.86,
            evidence: "bind-address=0.0.0.0".to_string(),
            line_number: 2,
        }],
        generated_at: "2026-01-01T00:00:00Z".to_string(),
        warnings: Vec::new(),
    };

    let value = serde_json::to_value(dto).unwrap();

    assert_eq!(value["journalCount"], 1);
    assert_eq!(value["textLogCount"], 15);
    assert_eq!(value["bashCommandCount"], 3);
    assert_eq!(value["sudoEventCount"], 6);
    assert_eq!(value["systemConfigCount"], 7);
    assert_eq!(value["webSiteCount"], 8);
    assert_eq!(value["webAccessLogCount"], 9);
    assert_eq!(value["webErrorLogCount"], 10);
    assert_eq!(value["webFindingCount"], 11);
    assert_eq!(value["mysqlConfigCount"], 12);
    assert_eq!(value["mysqlLogCount"], 13);
    assert_eq!(value["mysqlFindingCount"], 14);
    assert_eq!(value["totalCount"], 120);
    assert_eq!(value["truncated"], true);
    assert!((value["coverageRatio"].as_f64().unwrap() - 0.75).abs() < 0.000_001);
    assert_eq!(value["journalEntries"][0]["artifactId"], "artifact-1");
    assert_eq!(value["journalEntries"][0]["pid"], 42);
    assert_eq!(value["journalEntries"][0]["logKind"], "journald");
    assert_eq!(value["systemConfigs"][0]["configKind"], "passwdAccount");
    assert_eq!(value["systemConfigs"][0]["username"], "root");
    assert_eq!(value["webSites"][0]["serverKind"], "nginx");
    assert_eq!(value["webAccessLogs"][0]["clientIp"], "192.0.2.10");
    assert_eq!(value["webFindings"][0]["findingKind"], "sqlInjection");
    assert_eq!(value["mysqlConfigs"][0]["key"], "bind-address");
    assert_eq!(value["mysqlLogs"][0]["threadId"], "8");
    assert_eq!(value["mysqlFindings"][0]["findingKind"], "bindAddressAny");
    assert!(value["journalEntries"][0].get("executable").is_none());
    assert!(value.get("coverage_ratio").is_none());
    assert!(value.get("journal_count").is_none());
}

#[test]
fn linux_journal_entry_omits_log_kind_when_none() {
    let dto = LinuxJournalEntryDto {
        artifact_id: "artifact-3".to_string(),
        file_id: "file-3".to_string(),
        source_path: "/var/log/syslog".to_string(),
        timestamp: None,
        message: Some("line".to_string()),
        executable: None,
        systemd_unit: None,
        hostname: None,
        syslog_identifier: None,
        pid: None,
        priority: None,
        log_kind: None,
    };

    let value = serde_json::to_value(dto).unwrap();

    assert_eq!(value["artifactId"], "artifact-3");
    assert!(value.get("logKind").is_none());
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
