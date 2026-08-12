//! Integration tests for Linux artifact family wiring and removed macOS capability handling.

use app_services::analysis_service::{
    evidence_candidates_for_categories, extract_linux_candidate, run_analysis_extraction,
    AnalysisServiceError, EvidenceCandidate,
};
use domain::FileEntryId;
use transport::{ErrorCategory, ServiceErrorCategory};

fn candidate(path: &str) -> EvidenceCandidate {
    EvidenceCandidate {
        file_id: FileEntryId(format!("file-{}", path.replace(['/', '\\', '.'], "-"))),
        data_source_id: "ds-linux-test".to_string(),
        partition_index: None,
        path: path.to_string(),
        size: 1024,
        encrypted: false,
        content_identity: format!("test:{path}"),
        modified_at: None,
        evidence_kind: "test".to_string(),
        parser: "test".to_string(),
        category: "LinuxArtifacts".to_string(),
    }
}

fn candidate_with_mtime(path: &str, rfc3339_mtime: &str) -> EvidenceCandidate {
    let mut candidate = candidate(path);
    candidate.modified_at = Some(
        chrono::DateTime::parse_from_rfc3339(rfc3339_mtime)
            .expect("valid test mtime")
            .with_timezone(&chrono::Utc),
    );
    candidate
}

fn assert_has_outputs(
    outcome: &app_services::analysis_service::ExtractionOutcome,
    file_id: &FileEntryId,
) {
    assert!(
        !outcome.artifacts.is_empty() || !outcome.timeline_events.is_empty(),
        "extractor should produce at least one artifact or timeline event"
    );
    for artifact in &outcome.artifacts {
        assert_eq!(
            artifact.source_object_id.as_ref(),
            Some(file_id),
            "artifact {} is missing source_object_id",
            artifact.id.0
        );
    }
    for event in &outcome.timeline_events {
        assert_eq!(
            event.source_object_id, file_id.0,
            "timeline event {} has wrong source_object_id",
            event.id.0
        );
    }
}

#[test]
fn linux_bash_history_extraction_produces_events() {
    let candidate = candidate("/home/alice/.bash_history");
    let input = "#1700000000\nls -la /home\n";
    let outcome = extract_linux_candidate(&candidate, input.as_bytes());
    assert_has_outputs(&outcome, &candidate.file_id);
}

#[test]
fn linux_wtmp_extraction_produces_events() {
    let candidate = candidate("/var/log/wtmp");
    let mut buf = vec![0u8; 400];
    buf[0..4].copy_from_slice(&7i32.to_le_bytes());
    buf[4..8].copy_from_slice(&1234i32.to_le_bytes());
    let line = b"pts/0";
    buf[8..8 + line.len()].copy_from_slice(line);
    let user = b"alice";
    buf[44..44 + user.len()].copy_from_slice(user);
    let host = b"192.168.1.1";
    buf[76..76 + host.len()].copy_from_slice(host);
    buf[344..352].copy_from_slice(&1_700_000_000i64.to_le_bytes());
    buf[352..360].copy_from_slice(&0i64.to_le_bytes());

    let outcome = extract_linux_candidate(&candidate, &buf);
    assert_has_outputs(&outcome, &candidate.file_id);
}

#[test]
fn linux_apt_history_extraction_produces_events() {
    let candidate = candidate("/var/log/apt/history.log");
    let input = "Start-Date: 2024-01-15  10:30:00\nInstall: curl:amd64 (7.88.1)\nEnd-Date: 2024-01-15  10:30:15\n";
    let outcome = extract_linux_candidate(&candidate, input.as_bytes());
    assert_has_outputs(&outcome, &candidate.file_id);
}

#[test]
fn linux_dpkg_log_extraction_produces_events() {
    let candidate = candidate("/var/log/dpkg.log");
    let input = "2024-01-15 10:30:00 install curl:amd64 7.88.1\n";
    let outcome = extract_linux_candidate(&candidate, input.as_bytes());
    assert_has_outputs(&outcome, &candidate.file_id);
}

#[test]
fn linux_crontab_extraction_produces_artifacts() {
    let candidate = candidate("/etc/crontab");
    let input = "30 2 * * * /usr/bin/backup.sh\n";
    let outcome = extract_linux_candidate(&candidate, input.as_bytes());
    assert!(
        !outcome.artifacts.is_empty(),
        "crontab should produce at least one artifact"
    );
    assert_has_outputs(&outcome, &candidate.file_id);
}

#[test]
fn linux_sudo_log_extraction_produces_events() {
    let candidate = candidate("/var/log/auth.log");
    let input = "Jan 15 10:30:00 host sudo:   alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/apt update\n";
    let outcome = extract_linux_candidate(&candidate, input.as_bytes());
    assert_has_outputs(&outcome, &candidate.file_id);
}

#[test]
fn candidate_discovery_recognizes_linux_paths() {
    let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
    conn.execute(
        "CREATE TABLE file_entries (
            id TEXT PRIMARY KEY,
            data_source_id TEXT NOT NULL,
            path TEXT NOT NULL,
            size INTEGER,
            partition_index INTEGER,
            created_at TEXT,
            modified_at TEXT,
            accessed_at TEXT,
            changed_at TEXT,
            hash_sha256 TEXT,
            encrypted INTEGER CHECK (encrypted IS NULL OR encrypted IN (0, 1)),
            entry_type TEXT NOT NULL
        )",
        [],
    )
    .expect("create table");

    let paths = [
        "/var/log/wtmp",
        "/home/alice/.bash_history",
        "/var/log/apt/history.log",
        "/etc/crontab",
        "/var/log/auth.log",
    ];

    for (index, path) in paths.iter().enumerate() {
        conn.execute(
            "INSERT INTO file_entries
             (id, data_source_id, path, size, encrypted, entry_type)
             VALUES (?1, ?2, ?3, ?4, 0, 'file')",
            rusqlite::params![format!("file-{index}"), "ds-test", path, 1024i64],
        )
        .expect("insert");
    }

    let linux =
        evidence_candidates_for_categories(&conn, &["LinuxArtifacts"]).expect("discover linux");
    let linux_paths: Vec<_> = linux.iter().map(|item| item.path.as_str()).collect();
    assert!(linux_paths.contains(&"/var/log/wtmp"));
    assert!(linux_paths.contains(&"/home/alice/.bash_history"));
    assert!(linux_paths.contains(&"/var/log/apt/history.log"));
    assert!(linux_paths.contains(&"/etc/crontab"));
    assert!(linux_paths.contains(&"/var/log/auth.log"));
}

#[test]
fn removed_mac_artifact_candidate_request_is_typed_unsupported() {
    let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
    let error = evidence_candidates_for_categories(&conn, &[" MacArtifacts "])
        .expect_err("removed macOS capability must fail closed");

    assert!(matches!(
        &error,
        AnalysisServiceError::Unsupported(capability) if capability == "MacArtifacts"
    ));
    assert!(matches!(error.category(), ErrorCategory::Unsupported));
}

#[test]
fn removed_mac_artifact_extraction_request_is_typed_unsupported() {
    let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
    let error = run_analysis_extraction(
        &conn,
        "case-linux",
        domain::DataSourcePlatform::Linux,
        &["MacArtifacts"],
        |_| {
            Ok::<Box<dyn std::io::Read>, std::io::Error>(Box::new(std::io::Cursor::new(
                Vec::<u8>::new(),
            )))
        },
    )
    .expect_err("removed macOS capability must fail before evidence access");

    assert!(matches!(
        &error,
        AnalysisServiceError::Unsupported(capability) if capability == "MacArtifacts"
    ));
    assert!(matches!(error.category(), ErrorCategory::Unsupported));
}

fn artifact_attr<'a>(artifact: &'a domain::Artifact, key: &str) -> Option<&'a serde_json::Value> {
    artifact.attrs.get(key)
}

#[test]
fn linux_auth_log_dual_channel_covers_sudo_and_sshd_without_duplicates() {
    let candidate = candidate_with_mtime("/var/log/auth.log", "2024-03-10T00:00:00Z");
    let input = concat!(
        "Mar  5 12:00:00 host sshd[1234]: Accepted publickey for alice from 10.0.0.1 port 5555 ssh2\n",
        "Mar  5 12:01:00 host sudo:   alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/apt update\n",
        "Mar  5 12:02:00 host sshd[1234]: Failed password for invalid user bob from 10.0.0.2 port 6666 ssh2\n",
    );
    let outcome = extract_linux_candidate(&candidate, input.as_bytes());
    assert_has_outputs(&outcome, &candidate.file_id);

    let sudo_events: Vec<_> = outcome
        .artifacts
        .iter()
        .filter(|artifact| artifact.family == "LinuxSudoEvent")
        .collect();
    assert_eq!(sudo_events.len(), 1, "exactly one sudo event expected");
    assert_eq!(
        artifact_attr(sudo_events[0], "command").and_then(|value| value.as_str()),
        Some("/usr/bin/apt update")
    );

    let text_lines: Vec<_> = outcome
        .artifacts
        .iter()
        .filter(|artifact| artifact.family == "LinuxJournal")
        .collect();
    assert_eq!(
        text_lines.len(),
        2,
        "both sshd lines reach the text channel"
    );
    assert!(
        text_lines.iter().all(|artifact| {
            artifact_attr(artifact, "message")
                .and_then(|value| value.as_str())
                .is_some_and(|message| !message.contains("COMMAND="))
        }),
        "sudo lines must not be duplicated into the text-log channel"
    );
    assert!(
        text_lines.iter().any(|artifact| {
            artifact_attr(artifact, "message")
                .and_then(|value| value.as_str())
                .is_some_and(|message| message.contains("Accepted publickey"))
        }),
        "sshd accepted-publickey line must survive auth.log extraction"
    );
}

#[test]
fn linux_syslog_classic_timestamp_enters_timeline_with_mtime_year() {
    let candidate = candidate_with_mtime("/var/log/syslog", "2024-03-10T00:00:00Z");
    let input = "Mar  5 12:00:00 host cron[99]: job finished\n";
    let outcome = extract_linux_candidate(&candidate, input.as_bytes());
    assert_has_outputs(&outcome, &candidate.file_id);
    assert_eq!(outcome.timeline_events.len(), 1);
    assert_eq!(
        outcome.timeline_events[0].timestamp.to_rfc3339(),
        "2024-03-05T12:00:00+00:00"
    );
    let artifact = &outcome.artifacts[0];
    assert_eq!(
        artifact_attr(artifact, "timestamp").and_then(|value| value.as_str()),
        Some("2024-03-05T12:00:00+00:00")
    );
    assert_eq!(
        artifact_attr(artifact, "hostname").and_then(|value| value.as_str()),
        Some("host")
    );
    assert_eq!(
        artifact_attr(artifact, "syslogIdentifier").and_then(|value| value.as_str()),
        Some("cron")
    );
    assert_eq!(
        artifact_attr(artifact, "pid").and_then(|value| value.as_u64()),
        Some(99)
    );
}

#[test]
fn linux_syslog_classic_timestamp_rolls_back_year_after_reference() {
    // File modified in March 2024 still holds December lines from the tail of
    // the previous log year.
    let candidate = candidate_with_mtime("/var/log/messages", "2024-03-01T00:00:00Z");
    let input = "Dec 31 23:59:59 host kernel: year boundary\n";
    let outcome = extract_linux_candidate(&candidate, input.as_bytes());
    assert_eq!(outcome.timeline_events.len(), 1);
    assert_eq!(
        outcome.timeline_events[0].timestamp.to_rfc3339(),
        "2023-12-31T23:59:59+00:00"
    );
}

#[test]
fn linux_syslog_classic_timestamp_falls_back_to_rfc3339_year() {
    // No candidate mtime: the year comes from an RFC3339 line seen earlier in
    // the same file.
    let candidate = candidate("/var/log/syslog");
    let input = concat!(
        "2024-01-15T10:30:00+00:00 host app: structured line\n",
        "Mar  5 12:00:00 host svc: classic line\n",
    );
    let outcome = extract_linux_candidate(&candidate, input.as_bytes());
    assert_eq!(outcome.timeline_events.len(), 2);
    assert_eq!(
        outcome.timeline_events[1].timestamp.to_rfc3339(),
        "2024-03-05T12:00:00+00:00"
    );
}

#[test]
fn linux_syslog_unanchored_timestamp_keeps_date_in_attrs_and_warns() {
    let candidate = candidate("/var/log/syslog");
    let input = "Mar  5 12:00:00 host svc[7]: no year anchor available\n";
    let outcome = extract_linux_candidate(&candidate, input.as_bytes());
    assert_eq!(outcome.artifacts.len(), 1);
    assert!(
        outcome.timeline_events.is_empty(),
        "unanchored classic lines must not invent a timeline timestamp"
    );
    assert_eq!(
        artifact_attr(&outcome.artifacts[0], "logDate").and_then(|value| value.as_str()),
        Some("Mar 5 12:00:00")
    );
    assert!(
        outcome
            .warnings
            .iter()
            .any(|warning| warning.contains("no year anchor")),
        "missing year anchors must be counted in warnings: {:?}",
        outcome.warnings
    );
}

#[test]
fn linux_audit_log_epoch_timestamp_enters_timeline() {
    let candidate = candidate("/var/log/audit/audit.log");
    let input = "type=USER_LOGIN pid=1234 uid=0 msg=audit(1699999999.123:456): login ok\n";
    let outcome = extract_linux_candidate(&candidate, input.as_bytes());
    assert_has_outputs(&outcome, &candidate.file_id);
    assert_eq!(outcome.timeline_events.len(), 1);
    assert_eq!(
        outcome.timeline_events[0].timestamp.to_rfc3339(),
        "2023-11-14T22:13:19.123+00:00"
    );
    assert_eq!(
        artifact_attr(&outcome.artifacts[0], "priority").and_then(|value| value.as_u64()),
        Some(6)
    );
}

#[test]
fn linux_shadow_extraction_reports_password_state_without_hashes() {
    let candidate = candidate("/etc/shadow");
    let input = "root:$6$notarealhash:19000:0:99999:7:::\ndaemon:!:19000:0:99999:7:::\nalice::19000:0:99999:7:::\n";
    let outcome = extract_linux_candidate(&candidate, input.as_bytes());
    assert_eq!(outcome.artifacts.len(), 3);
    let root = outcome
        .artifacts
        .iter()
        .find(|artifact| {
            artifact_attr(artifact, "username").and_then(|value| value.as_str()) == Some("root")
        })
        .expect("root shadow record");
    assert_eq!(
        artifact_attr(root, "configKind").and_then(|value| value.as_str()),
        Some("shadowAccount")
    );
    assert_eq!(
        artifact_attr(root, "hasPassword").and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        artifact_attr(root, "locked").and_then(|value| value.as_bool()),
        Some(false)
    );
    let daemon = outcome
        .artifacts
        .iter()
        .find(|artifact| {
            artifact_attr(artifact, "username").and_then(|value| value.as_str()) == Some("daemon")
        })
        .expect("daemon shadow record");
    assert_eq!(
        artifact_attr(daemon, "locked").and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        artifact_attr(daemon, "hasPassword").and_then(|value| value.as_bool()),
        Some(false)
    );
    let serialized = serde_json::to_string(&outcome.artifacts).expect("serialize artifacts");
    assert!(
        !serialized.contains("notarealhash"),
        "password hashes must never reach artifact attributes"
    );
}

#[test]
fn linux_crontab_kind_follows_source_path() {
    let system = candidate("/etc/crontab");
    let outcome =
        extract_linux_candidate(&system, "30 2 * * * root /usr/bin/backup.sh\n".as_bytes());
    let job = outcome
        .artifacts
        .iter()
        .find(|artifact| artifact.family == "LinuxCronJob")
        .expect("system crontab job");
    assert_eq!(
        artifact_attr(job, "user").and_then(|value| value.as_str()),
        Some("root"),
        "system crontab entries carry an explicit user field"
    );

    let user = candidate("/var/spool/cron/alice");
    let outcome = extract_linux_candidate(&user, "30 2 * * * /usr/bin/backup.sh\n".as_bytes());
    let job = outcome
        .artifacts
        .iter()
        .find(|artifact| artifact.family == "LinuxCronJob")
        .expect("user crontab job");
    assert_eq!(
        artifact_attr(job, "user"),
        None,
        "user crontab entries must not invent a user field"
    );

    let nested = candidate("/var/spool/cron/atjobs/pending");
    let outcome = extract_linux_candidate(&nested, "anything\n".as_bytes());
    assert!(
        outcome
            .artifacts
            .iter()
            .all(|artifact| artifact.family != "LinuxCronJob"),
        "spool subdirectories are not user crontabs"
    );
}

#[test]
fn linux_web_vhost_logs_route_to_access_and_error_parsers() {
    let access = candidate("/var/log/apache2/other_vhosts_access.log");
    let input =
        "192.0.2.10 - - [15/Jan/2024:10:30:00 +0000] \"GET / HTTP/1.1\" 200 42 \"-\" \"curl/8\"\n";
    let outcome = extract_linux_candidate(&access, input.as_bytes());
    assert!(
        outcome
            .artifacts
            .iter()
            .any(|artifact| artifact.family == "LinuxWebAccessLog"),
        "vhost access log should produce LinuxWebAccessLog artifacts"
    );

    let error = candidate("/var/log/nginx/example.com.error.log");
    let input = "2024/01/15 10:30:00 [error] 123#0: *1 open() failed\n";
    let outcome = extract_linux_candidate(&error, input.as_bytes());
    let artifact = outcome
        .artifacts
        .iter()
        .find(|artifact| artifact.family == "LinuxWebErrorLog")
        .expect("vhost error log artifact");
    assert_eq!(
        artifact_attr(artifact, "timestamp").and_then(|value| value.as_str()),
        Some("2024-01-15T10:30:00+00:00")
    );
    assert_eq!(
        artifact_attr(artifact, "severity").and_then(|value| value.as_str()),
        Some("error")
    );
    assert_eq!(outcome.timeline_events.len(), 1);
}

#[test]
fn linux_apache_error_log_parses_timestamp_and_second_bracket_severity() {
    let candidate = candidate("/var/log/httpd/error_log");
    let input = "[Mon Jan 15 10:30:00.123456 2024] [core:error] [pid 123] [client 1.2.3.4] File does not exist\n";
    let outcome = extract_linux_candidate(&candidate, input.as_bytes());
    let artifact = outcome
        .artifacts
        .iter()
        .find(|artifact| artifact.family == "LinuxWebErrorLog")
        .expect("apache error log artifact");
    assert_eq!(
        artifact_attr(artifact, "timestamp").and_then(|value| value.as_str()),
        Some("2024-01-15T10:30:00.123456+00:00")
    );
    assert_eq!(
        artifact_attr(artifact, "severity").and_then(|value| value.as_str()),
        Some("core:error"),
        "Apache severity comes from the module:level bracket, not the timestamp bracket"
    );
    assert_eq!(outcome.timeline_events.len(), 1);
}

fn gzip_compress(input: &[u8]) -> Vec<u8> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(input).expect("gzip write");
    encoder.finish().expect("gzip finish")
}

#[test]
fn linux_truncated_gzip_yields_decoded_prefix_with_warning() {
    let mut lines = String::new();
    for index in 0..500 {
        lines.push_str(&format!("command-{index} --flag\n"));
    }
    let compressed = gzip_compress(lines.as_bytes());
    // Cut the trailer and part of the deflate stream: the read limit hitting a
    // rotated log mid-stream looks exactly like this to the decoder.
    let truncated = &compressed[..compressed.len() - 24];

    let candidate = candidate("/home/alice/.bash_history.gz");
    let outcome = extract_linux_candidate(&candidate, truncated);

    assert!(
        !outcome.artifacts.is_empty(),
        "a truncated gzip stream must still yield the decoded prefix"
    );
    assert!(
        outcome
            .warnings
            .iter()
            .any(|warning| warning.contains("gzip stream ends prematurely")),
        "truncated stream must be reported: {:?}",
        outcome.warnings
    );
}

#[test]
fn linux_corrupt_gzip_still_fails_closed() {
    let candidate = candidate("/home/alice/.bash_history.gz");
    let outcome = extract_linux_candidate(&candidate, b"definitely not a gzip stream");

    assert!(outcome.artifacts.is_empty());
    assert!(
        outcome
            .warnings
            .iter()
            .any(|warning| warning.contains("gzip decode failed")),
        "corrupt gzip must keep the failure warning: {:?}",
        outcome.warnings
    );
}

#[test]
fn linux_bash_history_respects_per_source_cap() {
    let mut input = String::new();
    for index in 0..20_001 {
        input.push_str(&format!("command-{index}\n"));
    }
    let candidate = candidate("/home/alice/.bash_history");
    let outcome = extract_linux_candidate(&candidate, input.as_bytes());

    assert_eq!(outcome.artifacts.len(), 20_000);
    assert!(
        outcome.warnings.iter().any(|warning| warning
            .contains("bash history emitted first 20000 records only (1 more skipped)")),
        "cap warning with skipped count expected: {:?}",
        outcome.warnings
    );
}

#[test]
fn linux_wtmp_respects_per_source_cap() {
    let mut record = vec![0u8; 400];
    record[0..4].copy_from_slice(&7i32.to_le_bytes());
    record[4..8].copy_from_slice(&1234i32.to_le_bytes());
    record[8..13].copy_from_slice(b"pts/0");
    record[44..49].copy_from_slice(b"alice");
    record[76..87].copy_from_slice(b"192.168.1.1");
    record[344..352].copy_from_slice(&1_700_000_000i64.to_le_bytes());

    let mut buf = Vec::with_capacity(record.len() * 20_001);
    for _ in 0..20_001 {
        buf.extend_from_slice(&record);
    }
    let candidate = candidate("/var/log/wtmp");
    let outcome = extract_linux_candidate(&candidate, &buf);

    assert_eq!(outcome.artifacts.len(), 20_000);
    assert!(
        outcome
            .warnings
            .iter()
            .any(|warning| warning
                .contains("wtmp emitted first 20000 records only (1 more skipped)")),
        "cap warning with skipped count expected: {:?}",
        outcome.warnings
    );
}

#[test]
fn linux_apt_history_respects_per_source_cap() {
    let packages = (0..20_001)
        .map(|index| format!("pkg{index}:amd64 (1.0)"))
        .collect::<Vec<_>>()
        .join(", ");
    let input = format!(
        "Start-Date: 2024-01-15  10:30:00\nInstall: {packages}\nEnd-Date: 2024-01-15  10:30:15\n"
    );
    let candidate = candidate("/var/log/apt/history.log");
    let outcome = extract_linux_candidate(&candidate, input.as_bytes());

    assert_eq!(outcome.artifacts.len(), 20_000);
    assert!(
        outcome.warnings.iter().any(|warning| warning
            .contains("package log emitted first 20000 records only (1 more skipped)")),
        "cap warning with skipped count expected: {:?}",
        outcome.warnings
    );
}
