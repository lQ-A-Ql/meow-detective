use super::*;
use crate::analysis_service::capability::LINUX_UMBRELLA_KEY;
use crate::analysis_service::extraction::candidate_processing::read_candidate_bytes_with_progress;
use crate::analysis_service::extraction::output_persistence::persist_outputs;
use crate::analysis_service::extraction::ExtractionOutcome;
use chrono::Utc;
use domain::{Artifact, ArtifactId, TimelineEvent, TimelineEventId};
use persistence_sqlite::repositories::{artifact_repo::ArtifactRepo, timeline_repo::TimelineRepo};
use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;

#[test]
fn owned_candidate_bytes_are_reused_without_a_second_content_copy() {
    let candidate = EvidenceCandidate {
        file_id: FileEntryId("owned-bytes".to_string()),
        data_source_id: "source-linux".to_string(),
        partition_index: None,
        path: "var/www/html/index.php".to_string(),
        size: 4,
        content_identity: "test:owned-bytes".to_string(),
        evidence_kind: "File".to_string(),
        parser: "XFS".to_string(),
        category: LINUX_UMBRELLA_KEY.to_string(),
    };
    let input = vec![1, 2, 3, 4];
    let input_ptr = input.as_ptr();
    let mut input = Some(input);

    let bytes = read_candidate_bytes_with_progress(
        &candidate,
        4,
        &AtomicBool::new(false),
        &mut |_, _| {
            Ok::<CandidateSource, String>(CandidateSource::Bytes(
                input.take().expect("candidate bytes requested once"),
            ))
        },
        |_| {},
    )
    .expect("read owned candidate bytes");

    assert_eq!(bytes, vec![1, 2, 3, 4]);
    assert_eq!(bytes.as_ptr(), input_ptr);
}

#[test]
fn candidate_reader_reports_monotonic_byte_progress() {
    let candidate = EvidenceCandidate {
        file_id: FileEntryId("streamed-bytes".to_string()),
        data_source_id: "source-linux".to_string(),
        partition_index: None,
        path: "var/log/messages".to_string(),
        size: 192 * 1024,
        content_identity: "test:streamed-bytes".to_string(),
        evidence_kind: "File".to_string(),
        parser: "XFS".to_string(),
        category: LINUX_UMBRELLA_KEY.to_string(),
    };
    let expected = vec![0x5a; candidate.size as usize];
    let mut expected = Some(expected);
    let mut byte_progress = Vec::new();

    let bytes = read_candidate_bytes_with_progress(
        &candidate,
        candidate.size as usize,
        &AtomicBool::new(false),
        &mut |_, _| {
            Ok::<CandidateSource, String>(CandidateSource::Reader(Box::new(std::io::Cursor::new(
                expected.take().expect("candidate requested once"),
            ))))
        },
        |bytes_read| byte_progress.push(bytes_read),
    )
    .expect("read streamed candidate bytes");

    assert_eq!(bytes.len(), candidate.size as usize);
    assert_eq!(byte_progress.last().copied(), Some(candidate.size as usize));
    assert!(byte_progress.windows(2).all(|pair| pair[0] < pair[1]));
}

fn output_candidate() -> EvidenceCandidate {
    EvidenceCandidate {
        file_id: FileEntryId("web-output".to_string()),
        data_source_id: "source-linux".to_string(),
        partition_index: None,
        path: "var/www/html/index.php".to_string(),
        size: 128,
        content_identity: "test:web-output".to_string(),
        evidence_kind: "File".to_string(),
        parser: "XFS".to_string(),
        category: LINUX_UMBRELLA_KEY.to_string(),
    }
}

fn artifact(id: &str, source_id: &str, extractor_id: &str, version: &str) -> Artifact {
    Artifact {
        id: ArtifactId(id.to_string()),
        family: "LinuxWebFinding".to_string(),
        title: id.to_string(),
        summary: "summary".to_string(),
        source_object_id: Some(FileEntryId(source_id.to_string())),
        extractor_id: Some(extractor_id.to_string()),
        extractor_version: Some(version.to_string()),
        confidence: Some(0.85),
        source_attribution: None,
        created_at: Utc::now(),
        attrs: BTreeMap::from([(
            "dataSourceId".to_string(),
            serde_json::Value::String("source-linux".to_string()),
        )]),
    }
}

fn event(id: &str, source_id: &str, parser_id: &str, version: &str) -> TimelineEvent {
    TimelineEvent {
        id: TimelineEventId(id.to_string()),
        source_object_id: source_id.to_string(),
        event_type: "linux.web".to_string(),
        timestamp: Utc::now(),
        title: id.to_string(),
        description: "description".to_string(),
        parser_id: Some(parser_id.to_string()),
        parser_version: Some(version.to_string()),
        confidence: Some(0.85),
        source_attribution: None,
        attrs: BTreeMap::new(),
    }
}

fn source_connection() -> rusqlite::Connection {
    let connection = persistence_sqlite::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&connection).expect("run source migrations");
    connection
        .execute(
            "INSERT INTO data_sources
             (id, case_id, name, kind, source_path, imported_at)
             VALUES (
                 'source-linux', 'case-1', 'Linux source', 'raw', 'test.raw',
                 '2026-07-18T00:00:00Z'
             )",
            [],
        )
        .expect("register source database owner");
    connection
}

#[test]
fn output_replacement_commits_artifacts_timeline_and_checkpoint_atomically() {
    let connection = source_connection();
    let candidate = output_candidate();
    ArtifactRepo::new(&connection)
        .insert_batch(
            &[artifact(
                "old-artifact",
                &candidate.file_id.0,
                "linux.web.old",
                "0.9.0",
            )],
            "case-1",
            &candidate.data_source_id,
        )
        .expect("insert old artifact");
    TimelineRepo::new(&connection)
        .insert_batch_with_case(
            &[event(
                "old-event",
                &candidate.file_id.0,
                "linux.web.old",
                "0.9.0",
            )],
            "case-1",
        )
        .expect("insert old event");
    let capability = crate::analysis_service::capability::find_capability("LinuxWebServices")
        .expect("web capability");
    let mut state = ExtractionState::new(&[capability]);
    state.record_outcome(
        capability,
        &candidate,
        ExtractionOutcome {
            artifacts: vec![artifact(
                "new-artifact",
                &candidate.file_id.0,
                "linux.web.current",
                crate::analysis_service::ANALYSIS_EXTRACTOR_VERSION,
            )],
            timeline_events: vec![event(
                "new-event",
                &candidate.file_id.0,
                "linux.web.current",
                crate::analysis_service::ANALYSIS_EXTRACTOR_VERSION,
            )],
            warnings: Vec::new(),
        },
    );

    persist_outputs(&connection, "case-1", &mut state).expect("replace analysis outputs");

    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM artifacts", [], |row| row
                .get::<_, i64>(0))
            .expect("count artifacts"),
        1
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM timeline_events", [], |row| row
                .get::<_, i64>(0))
            .expect("count timeline"),
        1
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM source_meta
                 WHERE key LIKE 'analysis_candidate_scan:complete:%'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .expect("count complete checkpoints"),
        1
    );
}

#[test]
fn output_replacement_rolls_back_when_timeline_insert_fails() {
    let connection = source_connection();
    let candidate = output_candidate();
    ArtifactRepo::new(&connection)
        .insert_batch(
            &[artifact(
                "old-artifact",
                &candidate.file_id.0,
                "linux.web.old",
                "0.9.0",
            )],
            "case-1",
            &candidate.data_source_id,
        )
        .expect("insert old artifact");
    TimelineRepo::new(&connection)
        .insert_batch_with_case(
            &[event(
                "old-event",
                &candidate.file_id.0,
                "linux.web.old",
                "0.9.0",
            )],
            "case-1",
        )
        .expect("insert old event");
    connection
        .execute_batch(
            "CREATE TRIGGER reject_new_timeline
             BEFORE INSERT ON timeline_events
             WHEN NEW.id = 'new-event'
             BEGIN
                 SELECT RAISE(ABORT, 'injected timeline failure');
             END;",
        )
        .expect("install timeline failure");
    let capability = crate::analysis_service::capability::find_capability("LinuxWebServices")
        .expect("web capability");
    let mut state = ExtractionState::new(&[capability]);
    state.record_outcome(
        capability,
        &candidate,
        ExtractionOutcome {
            artifacts: vec![artifact(
                "new-artifact",
                &candidate.file_id.0,
                "linux.web.current",
                crate::analysis_service::ANALYSIS_EXTRACTOR_VERSION,
            )],
            timeline_events: vec![event(
                "new-event",
                &candidate.file_id.0,
                "linux.web.current",
                crate::analysis_service::ANALYSIS_EXTRACTOR_VERSION,
            )],
            warnings: Vec::new(),
        },
    );

    persist_outputs(&connection, "case-1", &mut state)
        .expect_err("timeline failure must roll back the replacement");

    assert_eq!(
        connection
            .query_row("SELECT id FROM artifacts", [], |row| row
                .get::<_, String>(0))
            .expect("load retained artifact"),
        "old-artifact"
    );
    assert_eq!(
        connection
            .query_row("SELECT id FROM timeline_events", [], |row| row
                .get::<_, String>(0))
            .expect("load retained event"),
        "old-event"
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM source_meta
                 WHERE key LIKE 'analysis_candidate_scan:complete:%'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .expect("count complete checkpoints"),
        0
    );
}

#[test]
fn output_replacement_rolls_back_when_artifact_source_attribution_conflicts() {
    let connection = source_connection();
    let candidate = output_candidate();
    ArtifactRepo::new(&connection)
        .insert_batch(
            &[artifact(
                "old-artifact",
                &candidate.file_id.0,
                "linux.web.old",
                "0.9.0",
            )],
            "case-1",
            &candidate.data_source_id,
        )
        .expect("insert old artifact");
    TimelineRepo::new(&connection)
        .insert_batch_with_case(
            &[event(
                "old-event",
                &candidate.file_id.0,
                "linux.web.old",
                "0.9.0",
            )],
            "case-1",
        )
        .expect("insert old event");
    let capability = crate::analysis_service::capability::find_capability("LinuxWebServices")
        .expect("web capability");
    let mut conflicting_artifact = artifact(
        "new-artifact",
        &candidate.file_id.0,
        "linux.web.current",
        crate::analysis_service::ANALYSIS_EXTRACTOR_VERSION,
    );
    conflicting_artifact.attrs.insert(
        "dataSourceId".to_string(),
        serde_json::Value::String("source-windows".to_string()),
    );
    let mut state = ExtractionState::new(&[capability]);
    state.record_outcome(
        capability,
        &candidate,
        ExtractionOutcome {
            artifacts: vec![conflicting_artifact],
            timeline_events: vec![event(
                "new-event",
                &candidate.file_id.0,
                "linux.web.current",
                crate::analysis_service::ANALYSIS_EXTRACTOR_VERSION,
            )],
            warnings: Vec::new(),
        },
    );

    let error = persist_outputs(&connection, "case-1", &mut state)
        .expect_err("source attribution conflict must roll back the replacement");

    assert!(error.to_string().contains("source-windows"));
    assert!(error.to_string().contains("source-linux"));
    assert_eq!(
        connection
            .query_row("SELECT id FROM artifacts", [], |row| row
                .get::<_, String>(0))
            .expect("load retained artifact"),
        "old-artifact"
    );
    assert_eq!(
        connection
            .query_row("SELECT id FROM timeline_events", [], |row| row
                .get::<_, String>(0))
            .expect("load retained event"),
        "old-event"
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM source_meta
                 WHERE key LIKE 'analysis_candidate_scan:complete:%'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .expect("count complete checkpoints"),
        0
    );
}

#[test]
fn complete_checkpoint_is_ignored_after_persisted_output_tampering() {
    let connection = source_connection();
    let candidate = output_candidate();
    let capability = crate::analysis_service::capability::find_capability("LinuxWebServices")
        .expect("web capability");
    let mut state = ExtractionState::new(&[capability]);
    state.record_outcome(
        capability,
        &candidate,
        ExtractionOutcome {
            artifacts: vec![artifact(
                "new-artifact",
                &candidate.file_id.0,
                "linux.web.current",
                crate::analysis_service::ANALYSIS_EXTRACTOR_VERSION,
            )],
            timeline_events: vec![event(
                "new-event",
                &candidate.file_id.0,
                "linux.web.current",
                crate::analysis_service::ANALYSIS_EXTRACTOR_VERSION,
            )],
            warnings: Vec::new(),
        },
    );
    persist_outputs(&connection, "case-1", &mut state).expect("persist analysis outputs");

    let key = (
        candidate.file_id.0.clone(),
        capability.key.to_string(),
        candidate.size,
        candidate.content_identity.clone(),
    );
    assert!(existing_complete_scan_keys(&connection)
        .expect("validate complete checkpoint")
        .contains_key(&key));

    connection
        .execute(
            "UPDATE artifacts SET summary = 'tampered' WHERE id = 'new-artifact'",
            [],
        )
        .expect("tamper persisted artifact");

    assert!(!existing_complete_scan_keys(&connection)
        .expect("revalidate complete checkpoint")
        .contains_key(&key));
}

#[test]
fn complete_checkpoint_is_ignored_when_stale_producer_output_remains() {
    let connection = source_connection();
    let candidate = output_candidate();
    let capability = crate::analysis_service::capability::find_capability("LinuxWebServices")
        .expect("web capability");
    let mut state = ExtractionState::new(&[capability]);
    state.record_outcome(
        capability,
        &candidate,
        ExtractionOutcome {
            artifacts: vec![artifact(
                "current-artifact",
                &candidate.file_id.0,
                "linux.web.current",
                crate::analysis_service::ANALYSIS_EXTRACTOR_VERSION,
            )],
            warnings: Vec::new(),
            timeline_events: Vec::new(),
        },
    );
    persist_outputs(&connection, "case-1", &mut state).expect("persist current output");
    ArtifactRepo::new(&connection)
        .insert_batch(
            &[artifact(
                "stale-artifact",
                &candidate.file_id.0,
                "linux.web.old",
                "0.9.0",
            )],
            "case-1",
            &candidate.data_source_id,
        )
        .expect("insert stale output");

    let key = (
        candidate.file_id.0.clone(),
        capability.key.to_string(),
        candidate.size,
        candidate.content_identity.clone(),
    );
    assert!(!existing_complete_scan_keys(&connection)
        .expect("validate complete checkpoint")
        .contains_key(&key));
}

#[test]
fn internal_execution_reports_retryable_read_failures_without_checkpointing() {
    let connection = source_connection();
    connection
        .execute(
            "INSERT INTO file_entries (
                id, parent_id, data_source_id, path, name, entry_type, size,
                deleted, hidden, system
             ) VALUES (
                'read-failure', NULL, 'source-linux', 'var/www/html/index.php',
                'index.php', 'file', 128, 0, 0, 0
             )",
            [],
        )
        .expect("insert analysis candidate");
    let cancel = AtomicBool::new(false);
    let mut ignore_progress = |_update: super::ExtractionProgressUpdate| {};

    let execution = run_analysis_extraction_with_source(
        &connection,
        "case-1",
        DataSourcePlatform::Linux,
        &["LinuxWebServices"],
        &cancel,
        &mut ignore_progress,
        |_, _| Err::<CandidateSource, String>("injected evidence read failure".to_string()),
    )
    .expect("return retryable extraction result");

    assert_eq!(execution.retryable_failure_count, 1);
    assert_eq!(
        execution.dto.status,
        transport::dto::AnalysisParseStatusDto::Failed
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM source_meta
                 WHERE key LIKE 'analysis_candidate_scan:%'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .expect("count checkpoints"),
        0
    );
}
