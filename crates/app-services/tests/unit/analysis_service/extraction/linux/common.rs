use super::*;

fn test_candidate() -> EvidenceCandidate {
    EvidenceCandidate {
        file_id: domain::FileEntryId("file-log".to_string()),
        data_source_id: "ds-test".to_string(),
        partition_index: None,
        path: "/var/log/nginx/access.log".to_string(),
        size: 4096,
        encrypted: false,
        content_identity: "test:log".to_string(),
        companions: Vec::new(),
        modified_at: None,
        evidence_kind: "test".to_string(),
        parser: "test".to_string(),
        category: "LinuxArtifacts".to_string(),
    }
}

#[test]
fn all_lines_parsed_stays_quiet() {
    let mut warnings = Vec::new();
    warn_on_parse_gap(&test_candidate(), "web access log", 100, 100, &mut warnings);
    assert!(warnings.is_empty());
}

#[test]
fn few_unparsed_lines_below_minimum_stays_quiet() {
    let mut warnings = Vec::new();
    warn_on_parse_gap(&test_candidate(), "web access log", 20, 16, &mut warnings);
    assert!(warnings.is_empty());
}

#[test]
fn exactly_a_quarter_unparsed_stays_quiet() {
    let mut warnings = Vec::new();
    warn_on_parse_gap(&test_candidate(), "web access log", 20, 15, &mut warnings);
    assert!(warnings.is_empty());
}

#[test]
fn above_a_quarter_unparsed_warns_with_counts() {
    let mut warnings = Vec::new();
    warn_on_parse_gap(&test_candidate(), "web access log", 20, 14, &mut warnings);
    assert_eq!(warnings.len(), 1);
    assert!(
        warnings[0].contains("parsed 14 of 20 non-blank lines"),
        "unexpected warning: {}",
        warnings[0]
    );
}

#[test]
fn zero_records_from_non_blank_content_warns() {
    let mut warnings = Vec::new();
    warn_on_parse_gap(&test_candidate(), "web access log", 3, 0, &mut warnings);
    assert_eq!(warnings.len(), 1);
    assert!(
        warnings[0].contains("produced no records from 3 non-blank lines"),
        "unexpected warning: {}",
        warnings[0]
    );
}
