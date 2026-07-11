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
        path: path.to_string(),
        size: 1024,
        evidence_kind: "test".to_string(),
        parser: "test".to_string(),
        category: "LinuxArtifacts".to_string(),
    }
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
            "INSERT INTO file_entries (id, data_source_id, path, size, entry_type) VALUES (?1, ?2, ?3, ?4, 'file')",
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
