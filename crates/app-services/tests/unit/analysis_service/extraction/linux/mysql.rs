use super::*;

fn test_candidate(path: &str) -> EvidenceCandidate {
    EvidenceCandidate {
        file_id: domain::FileEntryId("file-mysql".to_string()),
        data_source_id: "ds-test".to_string(),
        partition_index: None,
        path: path.to_string(),
        size: 4096,
        encrypted: false,
        content_identity: "test:mysql".to_string(),
        companions: Vec::new(),
        modified_at: None,
        evidence_kind: "test".to_string(),
        parser: "test".to_string(),
        category: "LinuxArtifacts".to_string(),
    }
}

#[test]
fn mysql_log_entries_flow_without_parse_gap_warning() {
    let candidate = test_candidate("/var/log/mysql/error.log");
    let content = "2026-01-01T00:00:00.000000Z 0 [Note] Server hostname (bind-address): '*'; port: 3306\n2026-01-01T00:00:01.000000Z 8 [Warning] Access denied for user 'root'@'192.0.2.10'\n";
    let mut outcome = ExtractionOutcome::default();

    extract_log(
        &candidate,
        content.as_bytes(),
        &mut outcome,
        &LinuxLogTimeContext::utc(),
    );

    let log_entries = outcome
        .artifacts
        .iter()
        .filter(|artifact| artifact.family == "LinuxMysqlLogEntry")
        .count();
    assert_eq!(log_entries, 2);
    assert!(
        outcome.warnings.is_empty(),
        "unexpected warnings: {:?}",
        outcome.warnings
    );
}

#[test]
fn empty_mysql_log_produces_no_records_and_no_warnings() {
    let candidate = test_candidate("/var/log/mysql/error.log.1");
    let mut outcome = ExtractionOutcome::default();

    extract_log(
        &candidate,
        b"\n  \n",
        &mut outcome,
        &LinuxLogTimeContext::utc(),
    );

    assert!(outcome.artifacts.is_empty());
    assert!(outcome.warnings.is_empty());
}

fn shanghai_context() -> LinuxLogTimeContext {
    LinuxLogTimeContext::for_zone("Asia/Shanghai".parse().expect("valid zone"))
}

fn attr<'a>(artifact: &'a domain::Artifact, key: &str) -> Option<&'a str> {
    artifact.attrs.get(key).and_then(Value::as_str)
}

#[test]
fn iso_timestamp_with_offset_is_absolute_even_without_inferred_zone() {
    // MySQL 8 log_timestamps=SYSTEM records the offset; the entry joins the
    // timeline correctly even when the host zone could not be determined.
    let candidate = test_candidate("/var/log/mysql/error.log");
    let content = "2024-01-15T10:30:00.123456+08:00 8 [Note] ready for connections\n";
    let mut outcome = ExtractionOutcome::default();

    extract_log(
        &candidate,
        content.as_bytes(),
        &mut outcome,
        &LinuxLogTimeContext::utc(),
    );

    assert_eq!(outcome.timeline_events.len(), 1);
    assert_eq!(
        outcome.timeline_events[0].timestamp.to_rfc3339(),
        "2024-01-15T02:30:00.123456+00:00"
    );
    let artifact = &outcome.artifacts[0];
    assert_eq!(
        attr(artifact, "timestamp"),
        Some("2024-01-15T02:30:00.123456+00:00")
    );
    assert_eq!(attr(artifact, "tzAssumed"), None);
}

#[test]
fn naive_timestamp_converts_with_inferred_zone() {
    let candidate = test_candidate("/var/log/mysqld.log");
    let content = "2024-01-15 10:30:00 8 [Note] ready for connections\n";
    let mut outcome = ExtractionOutcome::default();

    extract_log(
        &candidate,
        content.as_bytes(),
        &mut outcome,
        &shanghai_context(),
    );

    assert_eq!(outcome.timeline_events.len(), 1);
    assert_eq!(
        outcome.timeline_events[0].timestamp.to_rfc3339(),
        "2024-01-15T02:30:00+00:00"
    );
    let artifact = &outcome.artifacts[0];
    assert_eq!(
        attr(artifact, "timestamp"),
        Some("2024-01-15T02:30:00+00:00")
    );
    assert_eq!(attr(artifact, "tzAssumed"), Some("Asia/Shanghai"));
}

#[test]
fn naive_timestamp_kept_raw_marked_unverified_without_inferred_zone() {
    let candidate = test_candidate("/var/log/mariadb/mariadb.log");
    let content = "2024-01-15 10:30:00 8 [Note] ready for connections\n";
    let mut outcome = ExtractionOutcome::default();

    extract_log(
        &candidate,
        content.as_bytes(),
        &mut outcome,
        &LinuxLogTimeContext::utc(),
    );

    assert!(
        outcome.timeline_events.is_empty(),
        "unverified local timestamps must not enter the timeline as fake UTC"
    );
    let artifact = &outcome.artifacts[0];
    assert_eq!(attr(artifact, "timestamp"), Some("2024-01-15 10:30:00"));
    assert_eq!(attr(artifact, "tzAssumed"), Some("unverified-timezone"));
    assert!(
        outcome
            .warnings
            .iter()
            .any(|warning| warning.contains("unverified-timezone")),
        "missing unverified-timezone warning: {:?}",
        outcome.warnings
    );
}

#[test]
fn legacy_short_year_timestamp_converts_with_inferred_zone() {
    let candidate = test_candidate("/var/log/mysqld.log");
    let content = "240815 10:30:00 [Note] Access denied for user 'root'@'localhost'\n";
    let mut outcome = ExtractionOutcome::default();

    extract_log(
        &candidate,
        content.as_bytes(),
        &mut outcome,
        &shanghai_context(),
    );

    assert_eq!(outcome.timeline_events.len(), 1);
    assert_eq!(
        outcome.timeline_events[0].timestamp.to_rfc3339(),
        "2024-08-15T02:30:00+00:00"
    );
}

#[test]
fn legacy_short_year_boundary_maps_to_21st_century() {
    // The legacy yymmdd format shipped 2003-2018; both ends of the two-digit
    // range resolve inside 2000-2099.
    for (raw, expected_year) in [("000101 00:00:00", 2000), ("991231 23:59:59", 2099)] {
        match parse_mysql_log_timestamp(raw) {
            Some(MysqlLogTimestamp::Local(naive)) => {
                assert_eq!(naive.format("%Y").to_string(), expected_year.to_string())
            }
            other => panic!("{raw} must parse as a local timestamp, got {other:?}"),
        }
    }
}
