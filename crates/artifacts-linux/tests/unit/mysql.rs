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
