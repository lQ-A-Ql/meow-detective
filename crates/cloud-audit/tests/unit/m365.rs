use super::*;

#[test]
fn parse_empty_data() {
    let result = parse_m365_audit_log("");
    assert!(result.is_err());
}

#[test]
fn parse_m365_csv_with_audit_data() {
    let csv = r#"CreationDate,UserIds,Operations,AuditData
2024-06-15T12:00:00,alice@contoso.com,FileDownloaded,"{""Operation"":""FileDownloaded"",""UserId"":""alice@contoso.com"",""ObjectId"":""https://contoso.sharepoint.com/sites/team/Shared%20Documents/report.docx""}"
2024-06-15T12:05:00,bob@contoso.com,UserLoggedIn,"{""Operation"":""UserLoggedIn"",""UserId"":""bob@contoso.com"",""ClientIP"":""203.0.113.42""}"#;

    let entries = parse_m365_audit_log(csv).expect("should parse");
    assert_eq!(entries.len(), 2);

    let first = entries
        .iter()
        .find(|e| e.action == "FileDownloaded")
        .expect("FileDownloaded entry not found");
    assert_eq!(first.principal.as_deref(), Some("alice@contoso.com"));
    assert!(first.target.is_some());

    let second = entries
        .iter()
        .find(|e| e.action == "UserLoggedIn")
        .expect("UserLoggedIn entry not found");
    assert_eq!(second.principal.as_deref(), Some("bob@contoso.com"));
}

#[test]
fn parse_m365_malformed_audit_data_not_fatal() {
    let csv = r#"CreationDate,UserIds,Operations,AuditData
2024-06-15T12:00:00,alice@contoso.com,FileAccessed,not-json"#;

    let entries = parse_m365_audit_log(csv).expect("should parse");
    assert_eq!(entries.len(), 1);
    // Malformed AuditData is not fatal; the Operation from the top-level column wins
    assert_eq!(entries[0].action, "FileAccessed");
}
