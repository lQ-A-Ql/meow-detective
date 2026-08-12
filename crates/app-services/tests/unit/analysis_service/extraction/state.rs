use super::*;
use chrono::Utc;
use domain::{Artifact, ArtifactId, FileEntryId};
use rusqlite::Connection;
use std::collections::BTreeMap;

fn warning_artifact(source_object_id: &FileEntryId) -> Artifact {
    Artifact {
        id: ArtifactId(uuid::Uuid::new_v4().to_string()),
        family: "LinuxWebFinding".to_string(),
        title: "finding".to_string(),
        summary: "summary".to_string(),
        source_object_id: Some(source_object_id.clone()),
        extractor_id: Some("analysis".to_string()),
        extractor_version: Some(ANALYSIS_EXTRACTOR_VERSION.to_string()),
        confidence: None,
        source_attribution: None,
        created_at: Utc::now(),
        attrs: BTreeMap::new(),
    }
}

#[test]
fn warning_with_artifact_checkpoint_remains_pending_until_atomic_persistence() {
    let conn = Connection::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&conn).expect("run source migrations");
    let capability = crate::analysis_service::capability::find_capability("LinuxWebServices")
        .expect("web capability");
    let mut state = ExtractionState::new(&[capability]);

    let candidate = EvidenceCandidate {
        file_id: FileEntryId("web-1".to_string()),
        data_source_id: "source-linux".to_string(),
        partition_index: None,
        path: "var/www/html/index.php".to_string(),
        size: 32,
        encrypted: false,
        content_identity: "test:web-1".to_string(),
        modified_at: None,
        evidence_kind: "File".to_string(),
        parser: "XFS".to_string(),
        category: crate::analysis_service::capability::LINUX_UMBRELLA_KEY.to_string(),
    };
    state.record_outcome(
        capability,
        &candidate,
        ExtractionOutcome {
            artifacts: vec![warning_artifact(&candidate.file_id)],
            warnings: vec!["deterministic parser warning".to_string()],
            ..ExtractionOutcome::default()
        },
    );

    let diagnostic_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM source_meta
             WHERE key LIKE 'analysis_candidate_scan:diagnostic:%'",
            [],
            |row| row.get(0),
        )
        .expect("count diagnostic checkpoints");
    assert_eq!(diagnostic_count, 0);
    assert_eq!(state.artifacts.len(), 1);
    assert_eq!(state.complete_scans.len(), 1);
    assert_eq!(state.replacements.len(), 1);
}

#[test]
fn checkpoint_replay_preserves_section_counts_and_warnings() {
    let conn = Connection::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&conn).expect("run source migrations");
    let capability = crate::analysis_service::capability::find_capability("LinuxWebServices")
        .expect("web capability");
    let mut state = ExtractionState::new(&[capability]);
    let scan = CompleteAnalysisCandidateScan {
        source_object_id: "web-1".to_string(),
        capability_key: capability.key.to_string(),
        extractor_version: ANALYSIS_EXTRACTOR_VERSION.to_string(),
        source_size: 32,
        content_identity: "test:web-1".to_string(),
        artifact_count: 4,
        timeline_event_count: 2,
        output_digest: "a".repeat(64),
        warnings: vec!["deterministic parser warning".to_string()],
    };

    state.replay_complete(capability, &scan);
    let dto = state
        .into_dto(&conn, "2026-07-18T00:00:00Z".to_string())
        .expect("build replay DTO");

    assert_eq!(dto.scanned_count, 1);
    assert_eq!(dto.checkpoint_hit_count, 1);
    assert_eq!(dto.timeline_event_count, 2);
    assert_eq!(dto.warnings, scan.warnings);
    assert_eq!(dto.sections.len(), 1);
    assert_eq!(dto.sections[0].scanned_count, 1);
    assert_eq!(dto.sections[0].artifact_count, 4);
    assert_eq!(dto.sections[0].timeline_event_count, 2);
}

#[test]
fn clean_checkpoint_replay_counts_the_scanned_candidate() {
    let conn = Connection::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&conn).expect("run source migrations");
    let capability = crate::analysis_service::capability::find_capability("LinuxWebServices")
        .expect("web capability");
    let mut state = ExtractionState::new(&[capability]);

    state.replay_clean(capability);
    let dto = state
        .into_dto(&conn, "2026-07-18T00:00:00Z".to_string())
        .expect("build replay DTO");

    assert_eq!(dto.scanned_count, 1);
    assert_eq!(dto.checkpoint_hit_count, 1);
    assert_eq!(dto.sections[0].scanned_count, 1);
}

#[test]
fn output_digest_is_canonical_across_insertion_order() {
    let source = FileEntryId("web-1".to_string());
    let mut first = warning_artifact(&source);
    first.title = "a".to_string();
    let mut second = warning_artifact(&source);
    second.title = "b".to_string();
    let forward = ExtractionOutcome {
        artifacts: vec![first.clone(), second.clone()],
        ..ExtractionOutcome::default()
    };
    let reverse = ExtractionOutcome {
        artifacts: vec![second, first],
        ..ExtractionOutcome::default()
    };

    assert_eq!(output_digest(&forward), output_digest(&reverse));
}

#[test]
fn output_digest_preserves_duplicate_multiplicity() {
    let source = FileEntryId("web-1".to_string());
    let artifact = warning_artifact(&source);
    let single = ExtractionOutcome {
        artifacts: vec![artifact.clone()],
        ..ExtractionOutcome::default()
    };
    let duplicate = ExtractionOutcome {
        artifacts: vec![artifact.clone(), artifact],
        ..ExtractionOutcome::default()
    };

    assert_ne!(output_digest(&single), output_digest(&duplicate));
}

#[test]
fn retryable_source_read_failure_does_not_create_a_checkpoint() {
    let capability = crate::analysis_service::capability::find_capability("Registry")
        .expect("registry capability");
    let mut state = ExtractionState::new(&[capability]);

    state.record_retryable_failure(
        capability,
        "BitLocker volume is locked; register a verified unlock first".to_string(),
    );

    assert_eq!(state.retryable_failure_count, 1);
    assert!(state.clean_scans.is_empty());
    assert!(state.diagnostic_scans.is_empty());
    assert!(state.complete_scans.is_empty());
    assert!(!state.has_pending_outputs());
}
