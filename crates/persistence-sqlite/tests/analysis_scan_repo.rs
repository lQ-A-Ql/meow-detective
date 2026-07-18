use persistence_sqlite::{
    open_in_memory,
    repositories::analysis_scan_repo::{
        AnalysisScanRepo, CleanAnalysisCandidateScan, CompleteAnalysisCandidateScan,
        DiagnosticAnalysisCandidateScan,
    },
    runner,
};

#[test]
fn clean_candidate_scan_round_trips_and_invalidates_on_size_change() {
    let connection = open_in_memory().expect("open source database");
    runner::run_source_all(&connection).expect("run source migrations");
    let repository = AnalysisScanRepo::new(&connection);
    let scan = CleanAnalysisCandidateScan {
        source_object_id: "inode-42".to_string(),
        capability_key: "LinuxWebServices".to_string(),
        extractor_version: "1.0.0".to_string(),
        source_size: 4096,
        content_identity: "content-v1".to_string(),
    };

    repository
        .insert_clean_batch(std::slice::from_ref(&scan))
        .expect("insert clean scan");

    assert!(repository
        .is_clean("inode-42", "LinuxWebServices", "1.0.0", 4096, "content-v1",)
        .expect("query clean scan"));
    assert!(!repository
        .is_clean("inode-42", "LinuxWebServices", "1.0.0", 8192, "content-v1",)
        .expect("query changed scan"));
    assert_eq!(
        repository
            .list_clean_for_version("1.0.0")
            .expect("list clean scans"),
        vec![scan]
    );
}

#[test]
fn diagnostic_candidate_scan_round_trips_with_warning_details() {
    let connection = open_in_memory().expect("open source database");
    runner::run_source_all(&connection).expect("run source migrations");
    let repository = AnalysisScanRepo::new(&connection);
    let scan = DiagnosticAnalysisCandidateScan {
        source_object_id: "inode-77".to_string(),
        capability_key: "LinuxSystemConfig".to_string(),
        extractor_version: "1.0.0".to_string(),
        source_size: 8192,
        content_identity: "content-v1".to_string(),
        warnings: vec![
            "unsupported SSH key material".to_string(),
            "manual review required".to_string(),
        ],
    };

    repository
        .insert_diagnostic_batch(std::slice::from_ref(&scan))
        .expect("insert diagnostic scan");

    assert_eq!(
        repository
            .list_diagnostics_for_version("1.0.0")
            .expect("list diagnostic scans"),
        vec![scan]
    );
    assert!(repository
        .list_diagnostics_for_version("2.0.0")
        .expect("list another extractor version")
        .is_empty());
}

#[test]
fn complete_candidate_scan_round_trips_output_identity() {
    let connection = open_in_memory().expect("open source database");
    runner::run_source_all(&connection).expect("run source migrations");
    let repository = AnalysisScanRepo::new(&connection);
    let scan = CompleteAnalysisCandidateScan {
        source_object_id: "inode-99".to_string(),
        capability_key: "LinuxWebServices".to_string(),
        extractor_version: "2.0.0".to_string(),
        source_size: 16384,
        content_identity: "content-v1".to_string(),
        artifact_count: 4,
        timeline_event_count: 2,
        output_digest: "a".repeat(64),
        warnings: vec!["partial parser warning".to_string()],
    };

    repository
        .insert_all_checkpoint_batch(&[], &[], std::slice::from_ref(&scan))
        .expect("insert complete scan");

    assert_eq!(
        repository
            .list_complete_for_version("2.0.0")
            .expect("list complete scans"),
        vec![scan]
    );
    assert!(repository
        .list_complete_for_version("1.0.0")
        .expect("list stale version")
        .is_empty());
}

#[test]
fn checkpoint_cache_is_optional_for_non_source_databases() {
    let connection = open_in_memory().expect("open application database");
    runner::run_all(&connection).expect("run application migrations");
    let repository = AnalysisScanRepo::new(&connection);
    let scan = CleanAnalysisCandidateScan {
        source_object_id: "file-1".to_string(),
        capability_key: "Email".to_string(),
        extractor_version: "1.0.0".to_string(),
        source_size: 128,
        content_identity: "content-v1".to_string(),
    };

    repository
        .insert_clean_batch(std::slice::from_ref(&scan))
        .expect("skip optional checkpoint persistence");

    assert!(!repository
        .is_clean("file-1", "Email", "1.0.0", 128, "content-v1")
        .expect("missing source metadata is a cache miss"));
    assert!(repository
        .list_clean_for_version("1.0.0")
        .expect("missing source metadata yields no checkpoints")
        .is_empty());
}
