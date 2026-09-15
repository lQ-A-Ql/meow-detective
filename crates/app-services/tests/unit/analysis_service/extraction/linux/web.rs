use super::*;

fn test_candidate(path: &str) -> EvidenceCandidate {
    EvidenceCandidate {
        file_id: domain::FileEntryId("file-web".to_string()),
        data_source_id: "ds-test".to_string(),
        partition_index: None,
        path: path.to_string(),
        size: 4096,
        encrypted: false,
        content_identity: "test:web".to_string(),
        companions: Vec::new(),
        modified_at: None,
        evidence_kind: "test".to_string(),
        parser: "test".to_string(),
        category: "LinuxArtifacts".to_string(),
    }
}

fn combined_line(ip: &str, uri: &str) -> String {
    format!(
        "{ip} - - [15/Jan/2024:10:30:45 +0000] \"GET {uri} HTTP/1.1\" 200 100 \"-\" \"curl/8\"\n"
    )
}

#[test]
fn access_log_without_parseable_lines_warns_instead_of_staying_silent() {
    let candidate = test_candidate("/var/log/nginx/access.log.1");
    let content = "garbage line one\ngarbage line two\ngarbage line three\n";
    let mut outcome = ExtractionOutcome::default();

    extract_access_log(&candidate, content.as_bytes(), &mut outcome);

    assert!(outcome.artifacts.is_empty());
    assert_eq!(outcome.warnings.len(), 1);
    assert!(
        outcome.warnings[0].contains("produced no records from 3 non-blank lines"),
        "unexpected warning: {}",
        outcome.warnings[0]
    );
}

#[test]
fn access_log_above_unparseable_threshold_warns_with_counts() {
    let candidate = test_candidate("/var/log/nginx/access.log");
    let mut content = String::new();
    for index in 0..6 {
        content.push_str(&combined_line("192.0.2.10", &format!("/ok-{index}")));
        content.push_str("this is not a log line\n");
    }
    let mut outcome = ExtractionOutcome::default();

    extract_access_log(&candidate, content.as_bytes(), &mut outcome);

    assert_eq!(outcome.artifacts.len(), 6);
    assert_eq!(outcome.warnings.len(), 1);
    assert!(
        outcome.warnings[0].contains("parsed 6 of 12 non-blank lines"),
        "unexpected warning: {}",
        outcome.warnings[0]
    );
}

#[test]
fn access_log_below_unparseable_threshold_stays_quiet() {
    let candidate = test_candidate("/var/log/nginx/access.log");
    let mut content = String::new();
    for index in 0..9 {
        content.push_str(&combined_line("192.0.2.10", &format!("/ok-{index}")));
    }
    content.push_str("one bad line\n");
    let mut outcome = ExtractionOutcome::default();

    extract_access_log(&candidate, content.as_bytes(), &mut outcome);

    assert_eq!(outcome.artifacts.len(), 9);
    assert!(
        outcome.warnings.is_empty(),
        "unexpected warnings: {:?}",
        outcome.warnings
    );
}

#[test]
fn vhost_prefixed_access_log_recovers_client_ip_and_annotates_records() {
    let candidate = test_candidate("/var/log/httpd/access_log");
    let content = "shop.example.com 198.51.100.23 - - [15/Jan/2024:10:30:45 +0000] \"GET /cart HTTP/1.1\" 200 512 \"-\" \"Mozilla/5.0\"\n";
    let mut outcome = ExtractionOutcome::default();

    extract_access_log(&candidate, content.as_bytes(), &mut outcome);

    assert_eq!(outcome.artifacts.len(), 1);
    let attrs = &outcome.artifacts[0].attrs;
    assert_eq!(
        attrs.get("clientIp").and_then(serde_json::Value::as_str),
        Some("198.51.100.23")
    );
    assert_eq!(
        attrs.get("vhost").and_then(serde_json::Value::as_str),
        Some("shop.example.com")
    );
    assert_eq!(outcome.warnings.len(), 1);
    assert!(
        outcome.warnings[0].contains("vhost-prefixed access log format"),
        "unexpected warning: {}",
        outcome.warnings[0]
    );
}

#[test]
fn error_log_fully_parsed_stays_quiet() {
    let candidate = test_candidate("/var/log/nginx/error.log");
    let content = "[Mon Jan 15 10:30:00.123456 2024] [core:error] something broke\n";
    let mut outcome = ExtractionOutcome::default();

    extract_error_log(
        &candidate,
        content.as_bytes(),
        &mut outcome,
        &shanghai_context(),
    );

    assert_eq!(outcome.artifacts.len(), 1);
    assert!(
        outcome.warnings.is_empty(),
        "unexpected warnings: {:?}",
        outcome.warnings
    );
}

fn shanghai_context() -> LinuxLogTimeContext {
    LinuxLogTimeContext::for_zone("Asia/Shanghai".parse().expect("valid zone"))
}

fn attr<'a>(artifact: &'a domain::Artifact, key: &str) -> Option<&'a str> {
    artifact.attrs.get(key).and_then(Value::as_str)
}

#[test]
fn nginx_error_timestamp_converts_with_inferred_zone() {
    let candidate = test_candidate("/var/log/nginx/error.log");
    let content = "2024/01/15 10:30:00 [error] 123#0: *1 open() failed\n";
    let mut outcome = ExtractionOutcome::default();

    extract_error_log(
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
fn apache_error_timestamp_converts_with_inferred_zone() {
    let candidate = test_candidate("/var/log/httpd/error_log");
    let content = "[Mon Jan 15 10:30:00.123456 2024] [core:error] [pid 123] File does not exist\n";
    let mut outcome = ExtractionOutcome::default();

    extract_error_log(
        &candidate,
        content.as_bytes(),
        &mut outcome,
        &shanghai_context(),
    );

    assert_eq!(outcome.timeline_events.len(), 1);
    assert_eq!(
        outcome.timeline_events[0].timestamp.to_rfc3339(),
        "2024-01-15T02:30:00.123456+00:00"
    );
}

#[test]
fn dst_zone_applies_historical_offset_to_error_timestamps() {
    let candidate = test_candidate("/var/log/nginx/error.log");
    let new_york = LinuxLogTimeContext::for_zone("America/New_York".parse().expect("valid zone"));
    // January: EST (UTC-5); July: EDT (UTC-4).
    for (content, expected) in [
        (
            "2024/01/15 12:00:00 [error] 1#0: winter\n",
            "2024-01-15T17:00:00+00:00",
        ),
        (
            "2024/07/15 12:00:00 [error] 1#0: summer\n",
            "2024-07-15T16:00:00+00:00",
        ),
    ] {
        let mut outcome = ExtractionOutcome::default();
        extract_error_log(&candidate, content.as_bytes(), &mut outcome, &new_york);
        assert_eq!(outcome.timeline_events.len(), 1);
        assert_eq!(outcome.timeline_events[0].timestamp.to_rfc3339(), expected);
    }
}

#[test]
fn undetermined_zone_keeps_raw_timestamp_marked_unverified() {
    let candidate = test_candidate("/var/log/nginx/error.log");
    let content = "2024/01/15 10:30:00 [error] 123#0: *1 open() failed\n";
    let mut outcome = ExtractionOutcome::default();

    extract_error_log(
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
    assert_eq!(attr(artifact, "timestamp"), Some("2024/01/15 10:30:00"));
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
fn line_without_timestamp_has_no_timestamp_attrs() {
    let candidate = test_candidate("/var/log/nginx/error.log");
    let content = "a plain error line without any timestamp\n";
    let mut outcome = ExtractionOutcome::default();

    extract_error_log(
        &candidate,
        content.as_bytes(),
        &mut outcome,
        &LinuxLogTimeContext::utc(),
    );

    let artifact = &outcome.artifacts[0];
    assert_eq!(attr(artifact, "timestamp"), None);
    assert_eq!(attr(artifact, "tzAssumed"), None);
    assert!(outcome.timeline_events.is_empty());
}
