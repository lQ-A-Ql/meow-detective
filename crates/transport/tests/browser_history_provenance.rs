use transport::dto::BrowserVisitDto;

#[test]
fn browser_history_preserves_integer_precision_and_optional_legacy_fields() {
    let value = serde_json::json!({"artifactId":"a","fileId":"f","sourcePath":"History","browser":"Chrome","profile":"Default",
        "url":"https://example.test/","title":"Example","visitCount":1,"recordKind":"visit","rowId":"9223372036854775806",
        "visitId":"9223372036854775806","rawTimestamp":"13300000000000001","rawTimeBasis":"webkitMicroseconds1601"});
    let parsed: BrowserVisitDto = serde_json::from_value(value).unwrap();
    assert_eq!(parsed.raw_timestamp.as_deref(), Some("13300000000000001"));
    assert_eq!(
        serde_json::to_value(parsed).unwrap()["rowId"],
        "9223372036854775806"
    );
    let legacy = serde_json::json!({"artifactId":"a","fileId":"f","sourcePath":"History","browser":"Chrome","profile":"Default",
        "url":"https://example.test/","title":"Example","visitCount":1});
    let parsed: BrowserVisitDto = serde_json::from_value(legacy).unwrap();
    assert!(parsed.record_kind.is_none());
    assert!(serde_json::to_value(parsed)
        .unwrap()
        .get("rawTimestamp")
        .is_none());
}
