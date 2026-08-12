use super::*;

fn test_candidate() -> EvidenceCandidate {
    EvidenceCandidate {
        file_id: domain::FileEntryId("file-journal".to_string()),
        data_source_id: "ds-test".to_string(),
        partition_index: None,
        path: "/var/log/journal/machine/system.journal".to_string(),
        size: 4096,
        encrypted: false,
        content_identity: "test:journal".to_string(),
        modified_at: None,
        evidence_kind: "test".to_string(),
        parser: "test".to_string(),
        category: "LinuxArtifacts".to_string(),
    }
}

#[test]
fn journal_entries_are_capped_with_skip_warning() {
    let candidate = test_candidate();
    let entries = vec![artifacts_linux::JournalEntry::default(); MAX_JOURNAL_EVENTS_PER_SOURCE + 7];
    let mut outcome = ExtractionOutcome::default();

    push_entries(&candidate, entries, &mut outcome);

    assert_eq!(outcome.artifacts.len(), MAX_JOURNAL_EVENTS_PER_SOURCE);
    assert_eq!(outcome.warnings.len(), 1);
    assert!(
        outcome.warnings[0].contains("journal emitted first 50000 records only (7 more skipped)"),
        "unexpected warning: {}",
        outcome.warnings[0]
    );
}

#[test]
fn journal_parse_diagnostics_surface_as_warnings() {
    let candidate = test_candidate();
    let parse = artifacts_linux::JournalParseOutcome {
        skipped_compressed: 3,
        skipped_corrupt: 2,
        hash_mismatches: 1,
        truncated: true,
        entry_limit_hit: true,
        ..Default::default()
    };
    let mut warnings = Vec::new();

    push_parse_diagnostics(&candidate, &parse, &mut warnings);

    assert_eq!(warnings.len(), 5);
    assert!(warnings
        .iter()
        .any(|warning| warning.contains("ends before its declared arena")));
    assert!(warnings
        .iter()
        .any(|warning| warning.contains("skipped 3 compressed payloads")));
    assert!(warnings
        .iter()
        .any(|warning| warning.contains("skipped 2 corrupt objects")));
    assert!(warnings
        .iter()
        .any(|warning| warning.contains("hash mismatches: 1")));
    assert!(warnings
        .iter()
        .any(|warning| warning.contains("parser entry cap")));
}
