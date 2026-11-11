use super::*;
use crate::analysis_service::capability::LINUX_UMBRELLA_KEY;
use crate::analysis_service::extraction::candidate_processing::read_candidate_bytes_with_progress;
use crate::analysis_service::extraction::ExtractionOutcome;
use chrono::Utc;
use domain::{
    Artifact, ArtifactId, DataSourceId, EntryType, FileEntry, FileEntryId, TimelineEvent,
    TimelineEventId,
};
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo, file_repo::FileRepo, timeline_repo::TimelineRepo,
};
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::ops::Deref;
use std::sync::atomic::AtomicBool;

#[test]
fn owned_candidate_bytes_are_reused_without_a_second_content_copy() {
    let candidate = EvidenceCandidate {
        file_id: FileEntryId("owned-bytes".to_string()),
        data_source_id: "source-linux".to_string(),
        partition_index: None,
        path: "var/www/html/index.php".to_string(),
        size: 4,
        encrypted: false,
        content_identity: "test:owned-bytes".to_string(),
        companions: Vec::new(),
        modified_at: None,
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
        encrypted: false,
        content_identity: "test:streamed-bytes".to_string(),
        companions: Vec::new(),
        modified_at: None,
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
        encrypted: false,
        content_identity: "test:web-output".to_string(),
        companions: Vec::new(),
        modified_at: None,
        evidence_kind: "File".to_string(),
        parser: "XFS".to_string(),
        category: LINUX_UMBRELLA_KEY.to_string(),
    }
}

fn persist_outcome_for_test(
    connection: &Connection,
    candidate: &EvidenceCandidate,
    capability: AnalysisCapability,
    outcome: ExtractionOutcome,
) -> Result<(), AnalysisServiceError> {
    let source_id =
        crate::analysis_service::extraction::output_persistence::resolve_output_data_source_id(
            connection,
        )?;
    let replacements = [AnalysisOutputReplacement {
        source_object_id: candidate.file_id.0.clone(),
        producer: AnalysisOutputProducer::Prefix(capability.producer_prefix()),
    }];
    let output = crate::analysis_service::extraction::native_output::begin_streaming(
        connection,
        &source_id,
        &replacements,
    )?;
    let observation = output.persist_outcome(outcome)?;
    let mut state = ExtractionState::new(&[capability]);
    state.record_persisted_outcome(capability, candidate, observation);
    output.rebuild_derived_metrics()?;
    let staged = output.seal()?;
    crate::analysis_service::extraction::output_persistence::commit_analysis_control(
        connection, &mut state, staged,
    )?;
    Ok(())
}

fn persist_plugin_outcome_for_test(
    connection: &Connection,
    candidate: &EvidenceCandidate,
    capability: AnalysisCapability,
    outcome: ExtractionOutcome,
    producer_ids: &[String],
) -> Result<(), AnalysisServiceError> {
    let source_id =
        crate::analysis_service::extraction::output_persistence::resolve_output_data_source_id(
            connection,
        )?;
    let replacements = producer_ids
        .iter()
        .map(|producer| AnalysisOutputReplacement {
            source_object_id: candidate.file_id.0.clone(),
            producer: AnalysisOutputProducer::ExtractorId(producer.clone()),
        })
        .collect::<Vec<_>>();
    let output = crate::analysis_service::extraction::native_output::begin_streaming(
        connection,
        &source_id,
        &replacements,
    )?;
    let observation = output.persist_outcome(outcome)?;
    let mut state = ExtractionState::new(&[capability]);
    state.record_persisted_plugin_outcome(capability, observation, Vec::new());
    output.rebuild_derived_metrics()?;
    let staged = output.seal()?;
    crate::analysis_service::extraction::output_persistence::commit_analysis_control(
        connection, &mut state, staged,
    )?;
    Ok(())
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
        event_type: "REGISTRY_HIVE_LAST_WRITE".to_string(),
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

struct SourceFixture {
    _root: tempfile::TempDir,
    connection: Connection,
}

impl Deref for SourceFixture {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        &self.connection
    }
}

fn source_connection() -> SourceFixture {
    let root = tempfile::tempdir().expect("create source fixture");
    let source_dir = root.path().join("case/sources/source-linux");
    std::fs::create_dir_all(&source_dir).expect("create source directory");
    let db_path = source_dir.join("source.db");
    let connection =
        persistence_sqlite::open_or_create_source(&db_path).expect("open source database");
    persistence_sqlite::source_fjall::configure_source_identity(&connection, "source-linux")
        .expect("configure source identity");
    persistence_sqlite::source_fjall::configure_source_fjall(&connection, &db_path)
        .expect("configure source storage");
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
    let store = persistence_fjall::open_shared_source_store(
        source_dir.join("source.fjall"),
        persistence_fjall::DatabaseConfig::default(),
    )
    .expect("open source store");
    persistence_fjall::seal_source_generation(&store, "source-linux", "fixture", &"a".repeat(64))
        .expect("seal fixture generation");
    drop(store);
    SourceFixture {
        _root: root,
        connection,
    }
}

fn native_files(connection: &Connection) -> Vec<Artifact> {
    ArtifactRepo::new(connection)
        .list_by_family(None)
        .expect("list native artifacts")
}

fn native_events(connection: &Connection) -> Vec<TimelineEvent> {
    TimelineRepo::new(connection)
        .query_filtered(0, 10_000, None, None, None)
        .expect("list native timeline")
}

fn native_artifact_ids(connection: &Connection) -> Vec<String> {
    let mut ids = native_files(connection)
        .into_iter()
        .map(|artifact| artifact.id.0)
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

fn native_event_ids(connection: &Connection) -> Vec<String> {
    let mut ids = native_events(connection)
        .into_iter()
        .map(|event| event.id.0)
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

fn native_event_exists(connection: &Connection, id: &str) -> bool {
    let store = persistence_sqlite::source_fjall::open_store(connection)
        .expect("open source store")
        .expect("configured source store");
    let source_id = persistence_sqlite::source_fjall::source_id(connection)
        .expect("read source identity")
        .expect("source identity");
    let repository = persistence_fjall::TimelineRecordStore::from_source_store((*store).clone());
    repository
        .get(&repository.read_snapshot(), &source_id, id)
        .expect("read native timeline event")
        .is_some()
}

fn native_artifact_exists(connection: &Connection, id: &str) -> bool {
    let store = persistence_sqlite::source_fjall::open_store(connection)
        .expect("open source store")
        .expect("configured source store");
    let source_id = persistence_sqlite::source_fjall::source_id(connection)
        .expect("read source identity")
        .expect("source identity");
    let repository = persistence_fjall::ArtifactRecordStore::from_source_store((*store).clone());
    repository
        .get(&repository.read_snapshot(), &source_id, id)
        .expect("read native artifact")
        .is_some()
}

fn tamper_native_artifact(connection: &Connection, artifact_id: &str) {
    let mut artifact = native_files(connection)
        .into_iter()
        .find(|artifact| artifact.id.0 == artifact_id)
        .expect("find native artifact");
    artifact.summary = "tampered".to_string();
    ArtifactRepo::new(connection)
        .insert_batch(&[artifact], "case-1", "source-linux")
        .expect("tamper native artifact");
}

fn inject_corrupt_timeline(connection: &Connection) {
    let store = persistence_sqlite::source_fjall::open_store_for_write(connection)
        .expect("open source store")
        .expect("configured source store");
    let record_key = persistence_fjall::encode_key(
        "timeline.record",
        &[
            persistence_fjall::KeyPart::Text("source-linux".to_string()),
            persistence_fjall::KeyPart::Text("corrupt-event".to_string()),
        ],
    )
    .expect("encode corrupt event key");
    let timestamp = native_events(connection)
        .into_iter()
        .next()
        .expect("fixture has an existing event")
        .timestamp;
    let sort_value = (timestamp.timestamp_micros() as u64) ^ (1_u64 << 63);
    let source_key = persistence_fjall::encode_key(
        "timeline.source",
        &[
            persistence_fjall::KeyPart::Text("source-linux".to_string()),
            persistence_fjall::KeyPart::InverseU64(sort_value),
            persistence_fjall::KeyPart::Text("corrupt-event".to_string()),
        ],
    )
    .expect("encode corrupt event source index");
    store
        .database()
        .write_batch(
            &[
                persistence_fjall::WriteAction::Put {
                    keyspace: persistence_fjall::KeyspaceName::TimelineRecords,
                    key: record_key.clone(),
                    value: b"corrupt".to_vec(),
                },
                persistence_fjall::WriteAction::Put {
                    keyspace: persistence_fjall::KeyspaceName::TimelineIndexes,
                    key: source_key,
                    value: record_key,
                },
            ],
            persistence_fjall::PersistMode::SyncAll,
        )
        .expect("inject corrupt timeline record");
}

fn insert_file(connection: &Connection, file: FileEntry) {
    FileRepo::new(connection)
        .insert_batch(&[file])
        .expect("insert native file catalog entry");
    reseal_native_generation(connection);
}

fn insert_partitioned_file(connection: &Connection, file: FileEntry, partition: usize) {
    persistence_sqlite::source_fjall::upsert_files_with_partition(connection, &[file], partition)
        .expect("insert partitioned native file catalog entry");
    reseal_native_generation(connection);
}

fn reseal_native_generation(connection: &Connection) {
    let store = persistence_sqlite::source_fjall::open_store_for_write(connection)
        .expect("open source store")
        .expect("configured source store");
    persistence_fjall::replace_source_generation(
        &store,
        "source-linux",
        "fixture-reseal",
        &"a".repeat(64),
    )
    .expect("reseal fixture generation");
}

fn file_entry(id: &str, path: &str, size: u64, encrypted: bool) -> FileEntry {
    FileEntry {
        id: FileEntryId(id.to_string()),
        parent_id: None,
        data_source_id: DataSourceId("source-linux".to_string()),
        path: path.to_string(),
        name: path.rsplit('/').next().unwrap_or(path).to_string(),
        entry_type: EntryType::File,
        size: Some(size),
        ext: path.rsplit_once('.').map(|(_, ext)| ext.to_string()),
        deleted: false,
        hidden: false,
        system: false,
        encrypted,
        read_only: false,
        archive: false,
        unix_mode: None,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    }
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
    persist_outcome_for_test(
        &connection,
        &candidate,
        capability,
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
    )
    .expect("replace analysis outputs");

    assert_eq!(
        ArtifactRepo::new(&connection)
            .count()
            .expect("count artifacts"),
        1
    );
    assert_eq!(
        TimelineRepo::new(&connection)
            .count()
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
    inject_corrupt_timeline(&connection);
    let capability = crate::analysis_service::capability::find_capability("LinuxWebServices")
        .expect("web capability");
    persist_outcome_for_test(
        &connection,
        &candidate,
        capability,
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
    )
    .expect_err("timeline failure must roll back the replacement");

    assert_eq!(
        native_artifact_ids(&connection),
        vec!["old-artifact".to_string()]
    );
    assert!(native_event_exists(&connection, "old-event"));
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
    let error = persist_outcome_for_test(
        &connection,
        &candidate,
        capability,
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
    )
    .expect_err("source attribution conflict must roll back the replacement");

    assert!(error.to_string().contains("source-windows"));
    assert!(error.to_string().contains("source-linux"));
    assert_eq!(
        native_artifact_ids(&connection),
        vec!["old-artifact".to_string()]
    );
    assert_eq!(native_event_ids(&connection), vec!["old-event".to_string()]);
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
    persist_outcome_for_test(
        &connection,
        &candidate,
        capability,
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
    )
    .expect("persist analysis outputs");

    let key = (
        candidate.file_id.0.clone(),
        capability.key.to_string(),
        candidate.size,
        candidate.content_identity.clone(),
    );
    assert!(existing_complete_scan_keys(&connection)
        .expect("validate complete checkpoint")
        .contains_key(&key));

    tamper_native_artifact(&connection, "new-artifact");

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
    persist_outcome_for_test(
        &connection,
        &candidate,
        capability,
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
    )
    .expect("persist current output");
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
    insert_file(
        &connection,
        file_entry("read-failure", "var/www/html/index.php", 128, false),
    );
    let cancel = AtomicBool::new(false);
    let mut updates = Vec::new();
    let mut collect_progress = |update: super::ExtractionProgressUpdate| updates.push(update);

    let execution = run_analysis_extraction_with_source(
        &connection,
        "case-1",
        DataSourcePlatform::Linux,
        &["LinuxWebServices"],
        &cancel,
        &mut collect_progress,
        |_, _| Err::<CandidateSource, String>("injected evidence read failure".to_string()),
    )
    .expect("return retryable extraction result");

    assert_eq!(execution.retryable_failure_count, 1);
    assert_eq!(
        execution.dto.status,
        transport::dto::AnalysisParseStatusDto::Failed
    );
    assert!(updates
        .iter()
        .any(|update| { update.phase == transport::dto::AnalysisExtractionPhaseDto::Failed }));
    assert!(updates
        .iter()
        .all(|update| { update.phase != transport::dto::AnalysisExtractionPhaseDto::Completed }));
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

fn insert_rbd_web_candidate(connection: &rusqlite::Connection, encrypted: bool) {
    connection
        .execute(
            "UPDATE data_sources SET kind = 'ceph_rbd' WHERE id = 'source-linux'",
            [],
        )
        .expect("mark source as derived RBD");
    let entry = file_entry("rbd-web", "var/www/html/index.php", 128, encrypted);
    insert_partitioned_file(connection, entry, 2);
}

#[test]
fn encrypted_rbd_candidate_never_opens_provider_and_removes_stale_outputs() {
    let connection = source_connection();
    insert_rbd_web_candidate(&connection, true);
    ArtifactRepo::new(&connection)
        .insert_batch(
            &[artifact(
                "stale-artifact",
                "rbd-web",
                "linux.web.old",
                "0.9.0",
            )],
            "case-1",
            "source-linux",
        )
        .expect("insert stale artifact from the pre-fix analysis path");
    TimelineRepo::new(&connection)
        .insert_batch_with_case(
            &[event("stale-event", "rbd-web", "linux.web.old", "0.9.0")],
            "case-1",
        )
        .expect("insert stale timeline output from the pre-fix analysis path");
    let cancel = AtomicBool::new(false);
    let mut ignore_progress = |_update: super::ExtractionProgressUpdate| {};
    let mut provider_calls = 0usize;

    let execution = run_analysis_extraction_with_source(
        &connection,
        "case-1",
        DataSourcePlatform::Linux,
        &["LinuxWebServices"],
        &cancel,
        &mut ignore_progress,
        |_, _| {
            provider_calls += 1;
            Ok::<CandidateSource, String>(CandidateSource::Bytes(b"ciphertext".to_vec()))
        },
    )
    .expect("return explicit unsupported extraction result");

    assert_eq!(
        provider_calls, 0,
        "encrypted evidence must not reach the RBD provider"
    );
    assert_eq!(execution.dto.artifact_count, 0);
    assert_eq!(execution.dto.timeline_event_count, 0);
    assert!(execution.dto.warnings.iter().any(|warning| {
        warning.contains("EFS-encrypted")
            && warning.contains("unsupported for analysis")
            && warning.contains("content was not read")
    }));
    assert!(execution
        .dto
        .warnings
        .iter()
        .all(|warning| !warning.contains("var/www/html/index.php")));
    assert_eq!(ArtifactRepo::new(&connection).count().unwrap(), 0);
    assert_eq!(TimelineRepo::new(&connection).count().unwrap(), 0);
}

#[test]
fn unencrypted_rbd_candidate_still_reads_through_provider() {
    let connection = source_connection();
    insert_rbd_web_candidate(&connection, false);
    let cancel = AtomicBool::new(false);
    let mut ignore_progress = |_update: super::ExtractionProgressUpdate| {};
    let mut provider_calls = 0usize;

    let execution = run_analysis_extraction_with_source(
        &connection,
        "case-1",
        DataSourcePlatform::Linux,
        &["LinuxWebServices"],
        &cancel,
        &mut ignore_progress,
        |_, _| {
            provider_calls += 1;
            Ok::<CandidateSource, String>(CandidateSource::Bytes(b"<?php echo 'ok'; ?>".to_vec()))
        },
    )
    .expect("extract ordinary RBD candidate");

    assert_eq!(provider_calls, 1);
    assert!(!execution
        .dto
        .warnings
        .iter()
        .any(|warning| warning.contains("EFS-encrypted")));
}

#[test]
fn encrypted_rbd_event_log_never_opens_seekable_provider() {
    let connection = source_connection();
    connection
        .execute(
            "UPDATE data_sources SET kind = 'ceph_rbd' WHERE id = 'source-linux'",
            [],
        )
        .expect("mark source as derived RBD");
    let entry = file_entry(
        "rbd-evtx",
        "Windows/System32/winevt/Logs/Security.evtx",
        4096,
        true,
    );
    insert_partitioned_file(&connection, entry, 2);
    let cancel = AtomicBool::new(false);
    let mut ignore_progress = |_update: super::ExtractionProgressUpdate| {};
    let mut provider_calls = 0usize;

    let execution = run_analysis_extraction_with_source(
        &connection,
        "case-1",
        DataSourcePlatform::Windows,
        &["EventLogs"],
        &cancel,
        &mut ignore_progress,
        |_, _| {
            provider_calls += 1;
            Ok::<CandidateSource, String>(CandidateSource::Seekable(Box::new(
                std::io::Cursor::new(vec![0u8; 4096]),
            )))
        },
    )
    .expect("return explicit unsupported EVTX extraction result");

    assert_eq!(provider_calls, 0);
    assert_eq!(execution.dto.artifact_count, 0);
    assert_eq!(execution.dto.timeline_event_count, 0);
    assert!(execution
        .dto
        .warnings
        .iter()
        .any(|warning| warning.contains("EFS-encrypted")));
}

#[test]
fn seekable_evtx_persists_atomically_and_replays_complete_checkpoint() {
    let connection = source_connection();
    let bytes = std::fs::read(testing::fixtures::tiny_system_evtx())
        .expect("read public System.evtx fixture");
    insert_event_log_candidate(&connection, bytes.len() as u64);
    let cancel = AtomicBool::new(false);
    let mut ignore_progress = |_update: super::ExtractionProgressUpdate| {};

    let first = run_analysis_extraction_with_source(
        &connection,
        "case-1",
        DataSourcePlatform::Windows,
        &["EventLogs"],
        &cancel,
        &mut ignore_progress,
        |_, _| {
            Ok::<CandidateSource, String>(CandidateSource::Seekable(Box::new(
                std::io::Cursor::new(bytes.clone()),
            )))
        },
    )
    .expect("stream and persist EVTX outputs");

    assert_eq!(first.dto.scanned_count, 1);
    assert!(first.dto.artifact_count > 0);
    assert_eq!(first.dto.timeline_event_count, 0);
    let mut provider_calls = 0usize;
    let second = run_analysis_extraction_with_source(
        &connection,
        "case-1",
        DataSourcePlatform::Windows,
        &["EventLogs"],
        &cancel,
        &mut ignore_progress,
        |_, _| {
            provider_calls += 1;
            Err::<CandidateSource, String>("checkpoint replay must not read evidence".to_string())
        },
    )
    .expect("replay complete EVTX checkpoint");

    assert_eq!(provider_calls, 0);
    assert_eq!(second.dto.checkpoint_hit_count, 1);
    assert_eq!(second.dto.artifact_count, first.dto.artifact_count);
    assert_eq!(
        second.dto.timeline_event_count,
        first.dto.timeline_event_count
    );
}

#[test]
fn seekable_evtx_insert_failure_rolls_back_output_replacement() {
    let connection = source_connection();
    let bytes = std::fs::read(testing::fixtures::tiny_system_evtx())
        .expect("read public System.evtx fixture");
    insert_event_log_candidate(&connection, bytes.len() as u64);
    ArtifactRepo::new(&connection)
        .insert_batch(
            &[artifact(
                "old-evtx-artifact",
                "system-evtx",
                "evtx.structured.old",
                "0.9.0",
            )],
            "case-1",
            "source-linux",
        )
        .expect("insert previous EVTX artifact");
    TimelineRepo::new(&connection)
        .insert_batch_with_case(
            &[event(
                "old-evtx-event",
                "system-evtx",
                "evtx.structured.old",
                "0.9.0",
            )],
            "case-1",
        )
        .expect("insert previous EVTX event");
    inject_corrupt_timeline(&connection);
    let cancel = AtomicBool::new(false);
    let mut ignore_progress = |_update: super::ExtractionProgressUpdate| {};

    let result = run_analysis_extraction_with_source(
        &connection,
        "case-1",
        DataSourcePlatform::Windows,
        &["EventLogs"],
        &cancel,
        &mut ignore_progress,
        |_, _| {
            Ok::<CandidateSource, String>(CandidateSource::Seekable(Box::new(
                std::io::Cursor::new(bytes.clone()),
            )))
        },
    );
    assert!(
        result.is_err(),
        "injected batch failure must abort extraction"
    );

    assert!(native_artifact_exists(&connection, "old-evtx-artifact"));
    assert!(native_event_exists(&connection, "old-evtx-event"));
}

#[test]
fn seekable_evtx_reparses_checkpoint_from_before_ntfs_valid_data_fix() {
    let connection = source_connection();
    let bytes = std::fs::read(testing::fixtures::tiny_system_evtx()).unwrap();
    insert_event_log_candidate(&connection, bytes.len() as u64);
    let cancel = AtomicBool::new(false);
    let mut ignore_progress = |_update: super::ExtractionProgressUpdate| {};
    let first = run_analysis_extraction_with_source(
        &connection,
        "case-1",
        DataSourcePlatform::Windows,
        &["EventLogs"],
        &cancel,
        &mut ignore_progress,
        |_, _| {
            Ok::<CandidateSource, String>(CandidateSource::Seekable(Box::new(
                std::io::Cursor::new(bytes.clone()),
            )))
        },
    )
    .unwrap();
    let repository =
        persistence_sqlite::repositories::analysis_scan_repo::AnalysisScanRepo::new(&connection);
    let mut previous = repository
        .list_complete_for_version(crate::analysis_service::ANALYSIS_EXTRACTOR_VERSION)
        .unwrap()
        .pop()
        .expect("complete EVTX checkpoint");
    connection
        .execute(
            "DELETE FROM source_meta WHERE key LIKE 'analysis_candidate_scan:complete:%'",
            [],
        )
        .unwrap();
    previous.extractor_version = "1.3.0".to_string();
    previous.warnings = vec!["[EVTX-CHUNK] System.evtx: uninitialized allocation tail".to_string()];
    repository
        .insert_all_checkpoint_batch(&[], &[], &[previous])
        .unwrap();
    let mut provider_calls = 0;
    let second = run_analysis_extraction_with_source(
        &connection,
        "case-1",
        DataSourcePlatform::Windows,
        &["EventLogs"],
        &cancel,
        &mut ignore_progress,
        |_, _| {
            provider_calls += 1;
            Ok::<CandidateSource, String>(CandidateSource::Seekable(Box::new(
                std::io::Cursor::new(bytes.clone()),
            )))
        },
    )
    .unwrap();
    assert_eq!(provider_calls, 1);
    assert_eq!(second.dto.checkpoint_hit_count, 0);
    assert!(second.dto.warnings.is_empty());
    assert_eq!(second.dto.artifact_count, first.dto.artifact_count);
    assert_eq!(
        second.dto.timeline_event_count,
        first.dto.timeline_event_count
    );
    assert_eq!(
        ArtifactRepo::new(&connection).count().unwrap(),
        first.dto.artifact_count
    );
}

fn insert_event_log_candidate(connection: &Connection, size: u64) {
    let entry = file_entry(
        "system-evtx",
        "Windows/System32/winevt/Logs/System.evtx",
        size,
        false,
    );
    insert_partitioned_file(connection, entry, 2);
}

fn insert_plugin_shaped_candidate(connection: &Connection) {
    insert_file(
        connection,
        file_entry("plugin-mfx", "[P0]/Evidence/FOO.MFX", 64, false),
    );
}

#[test]
fn plugin_capability_stays_zero_overhead_without_loaded_plugins() {
    let connection = source_connection();
    insert_plugin_shaped_candidate(&connection);
    let cancel = AtomicBool::new(false);
    let loader_calls = std::cell::Cell::new(0u32);
    let plugin_loader = || {
        loader_calls.set(loader_calls.get() + 1);
        crate::plugin_loader::PluginLoadReport::default()
    };
    let mut ignore_progress = |_update: super::ExtractionProgressUpdate| {};

    let execution = run_analysis_extraction_with_plugin_loader(
        &connection,
        "case-1",
        DataSourcePlatform::Windows,
        &[],
        &cancel,
        &mut ignore_progress,
        |_, _| Ok::<CandidateSource, String>(CandidateSource::Bytes(vec![0u8; 16])),
        &plugin_loader,
    )
    .expect("run without plugins");

    assert_eq!(loader_calls.get(), 1, "loader runs once for a default run");
    assert!(execution
        .dto
        .sections
        .iter()
        .all(|section| section.key != "PluginArtifacts"));
    assert!(execution.plugin_loads.is_empty());
    assert!(execution.plugin_rejections.is_empty());
    assert!(execution.plugin_extract_failures.is_empty());
    assert_eq!(ArtifactRepo::new(&connection).count().unwrap(), 0);
}

#[test]
fn plugin_loader_is_never_invoked_when_capability_not_selected() {
    let connection = source_connection();
    let cancel = AtomicBool::new(false);
    let plugin_loader = || -> crate::plugin_loader::PluginLoadReport {
        panic!("plugin loader must not run for built-in-only selections")
    };
    let mut ignore_progress = |_update: super::ExtractionProgressUpdate| {};

    let execution = run_analysis_extraction_with_plugin_loader(
        &connection,
        "case-1",
        DataSourcePlatform::Windows,
        &["Registry"],
        &cancel,
        &mut ignore_progress,
        |_, _| Ok::<CandidateSource, String>(CandidateSource::Bytes(vec![0u8; 16])),
        &plugin_loader,
    )
    .expect("built-in-only run");

    assert_eq!(execution.dto.sections.len(), 1);
    assert_eq!(execution.dto.sections[0].key, "Registry");
}

#[test]
fn plugin_reextraction_replaces_outputs_by_exact_extractor_id() {
    let connection = source_connection();
    let candidate = output_candidate();
    ArtifactRepo::new(&connection)
        .insert_batch(
            &[
                artifact(
                    "old-plugin-artifact",
                    "web-output",
                    "meow.fixture.good",
                    "0.1.0",
                ),
                artifact("keep-builtin", "web-output", "linux.web.old", "0.9.0"),
                artifact(
                    "other-file-plugin",
                    "other-file",
                    "meow.fixture.good",
                    "0.1.0",
                ),
            ],
            "case-1",
            &candidate.data_source_id,
        )
        .expect("seed previous run outputs");
    TimelineRepo::new(&connection)
        .insert_batch_with_case(
            &[event(
                "old-plugin-event",
                "web-output",
                "meow.fixture.good",
                "0.1.0",
            )],
            "case-1",
        )
        .expect("seed previous run events");

    let capability = crate::analysis_service::capability::find_capability("PluginArtifacts")
        .expect("plugin capability");
    persist_plugin_outcome_for_test(
        &connection,
        &candidate,
        capability,
        ExtractionOutcome {
            artifacts: vec![artifact(
                "new-plugin-artifact",
                "web-output",
                "meow.fixture.good",
                "0.2.0",
            )],
            timeline_events: vec![event(
                "new-plugin-event",
                "web-output",
                "meow.fixture.good",
                "0.2.0",
            )],
            warnings: Vec::new(),
        },
        &["meow.fixture.good".to_string()],
    )
    .expect("persist plugin outputs");

    let artifact_ids = native_artifact_ids(&connection);
    assert_eq!(
        artifact_ids,
        vec![
            "keep-builtin".to_string(),
            "new-plugin-artifact".to_string(),
            "other-file-plugin".to_string()
        ],
        "exact-id replacement drops only this source's outputs by this plugin"
    );
    let event_ids = native_event_ids(&connection);
    assert_eq!(event_ids, vec!["new-plugin-event".to_string()]);

    let checkpoint_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM source_meta WHERE key LIKE 'analysis_candidate_scan:%'",
            [],
            |row| row.get(0),
        )
        .expect("count checkpoints");
    assert_eq!(
        checkpoint_count, 0,
        "plugin candidates record no checkpoints"
    );
}
