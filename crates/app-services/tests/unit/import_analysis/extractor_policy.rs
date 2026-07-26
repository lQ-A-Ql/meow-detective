use super::*;

fn prefetch_entry() -> FileEntry {
    FileEntry {
        id: FileEntryId("prefetch-1".to_string()),
        parent_id: None,
        data_source_id: domain::DataSourceId("ds-1".to_string()),
        path: "Windows/Prefetch/CMD.EXE-12345678.pf".to_string(),
        name: "CMD.EXE-12345678.pf".to_string(),
        entry_type: domain::EntryType::File,
        size: Some(4096),
        ext: Some("pf".to_string()),
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    }
}

#[test]
fn windows_policy_enables_registered_windows_extractors() {
    let policy =
        PlatformExtractorPolicy::for_platform(DataSourcePlatform::Windows).expect("Windows policy");

    assert!(policy.should_extract(&prefetch_entry()));
}

#[test]
fn windows_policy_skips_efs_encrypted_candidates() {
    let policy =
        PlatformExtractorPolicy::for_platform(DataSourcePlatform::Windows).expect("Windows policy");
    let mut encrypted = prefetch_entry();
    encrypted.encrypted = true;

    assert!(!policy.should_extract(&encrypted));
}

#[test]
fn linux_policy_does_not_expose_windows_extractors() {
    let policy =
        PlatformExtractorPolicy::for_platform(DataSourcePlatform::Linux).expect("Linux policy");

    assert!(!policy.should_extract(&prefetch_entry()));
}

#[test]
fn unknown_policy_fails_closed() {
    let error = match PlatformExtractorPolicy::for_platform(DataSourcePlatform::Unknown) {
        Ok(_) => panic!("unknown platform unexpectedly produced an extractor policy"),
        Err(error) => error,
    };

    assert!(matches!(
        error,
        ImportAnalysisError::UnsupportedPlatform(ref value) if value == "unknown"
    ));
}
