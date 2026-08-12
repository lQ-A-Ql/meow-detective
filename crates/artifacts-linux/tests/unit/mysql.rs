use super::*;

#[test]
fn parses_mysql_config_and_detects_risky_settings() {
    let content = r#"
[mysqld]
bind-address = 0.0.0.0
local_infile=ON
secure_file_priv=
general_log=1
"#;
    let entries = parse_mysql_config(content).unwrap();
    let findings = detect_mysql_config_findings(&entries);
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0].section.as_deref(), Some("mysqld"));
    assert_eq!(entries[1].key, "local-infile");
    assert!(findings.iter().any(|f| f.finding_kind == "bindAddressAny"));
    assert!(findings
        .iter()
        .any(|f| f.finding_kind == "localInfileEnabled"));
    assert!(findings
        .iter()
        .any(|f| f.finding_kind == "secureFilePrivEmpty"));
}

#[test]
fn parses_mysql_log_and_detects_auth_failure() {
    let content =
        "2026-01-01T00:00:00.000000Z 8 [Warning] Access denied for user 'root'@'192.0.2.10'\n";
    let entries = parse_mysql_log(content).unwrap();
    let findings = detect_mysql_log_findings(&entries);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].thread_id.as_deref(), Some("8"));
    assert_eq!(entries[0].severity.as_deref(), Some("warning"));
    assert_eq!(findings[0].finding_kind, "accessDenied");
}

#[test]
fn parses_legacy_short_year_timestamp() {
    let content = "240815 10:30:00 [Note] Access denied for user 'root'@'localhost'\n";
    let entries = parse_mysql_log(content).unwrap();
    assert_eq!(entries.len(), 1);
    let ts = entries[0].timestamp.expect("legacy timestamp must parse");
    assert_eq!(ts.to_rfc3339(), "2024-08-15T10:30:00+00:00");
    assert_eq!(entries[0].severity.as_deref(), Some("note"));
}

#[test]
fn rejects_invalid_legacy_timestamp_candidate() {
    // Digits/colon shape that is not a real date must fall through to no
    // timestamp instead of panicking or mis-parsing.
    let content = "249999 99:99:99 some message\n";
    let entries = parse_mysql_log(content).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].timestamp, None);
    assert!(entries[0].message.ends_with("some message"));
}
