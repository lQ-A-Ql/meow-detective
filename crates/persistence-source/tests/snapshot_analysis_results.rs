use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

use chrono::Utc;
use domain::{
    structure::{
        AcquisitionId, SnapshotAnalysisReceipt, SourceRevisionContent, SourceRevisionRef,
        StructureAttemptRef,
    },
    CaseId, CaseMeta, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance,
};
use persistence_source::{
    analysis_results::{
        read_analysis_result, write_analysis_result, AnalysisResultError, AnalysisResultRef,
    },
    control::{open_source_db, source_db_path},
    fjall::{
        encode_key, read_source_generation_manifest, seal_source_generation_with_control,
        source_generation_manifest_digest, KeyPart, KeyspaceName, PersistMode, SourceStore,
        VersionedEnvelope, SOURCE_GENERATION_MANIFEST_FILE, SOURCE_GENERATION_SEAL_FILE,
    },
};
use persistence_sqlite::repositories::{
    case_repo::CaseRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    source_revision_repo::SourceRevisionRepo,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

struct Fixture {
    root: tempfile::TempDir,
    connection: rusqlite::Connection,
    case: CaseId,
}

impl Fixture {
    fn new() -> Self {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-workspaces/snapshot-analysis-results");
        std::fs::create_dir_all(&workspace).unwrap();
        let root = tempfile::tempdir_in(workspace).unwrap();
        let connection = persistence_sqlite::open_or_create(&root.path().join("app.db")).unwrap();
        persistence_sqlite::runner::run_all(&connection).unwrap();
        let case = CaseId(uuid::Uuid::new_v4().to_string());
        CaseRepo::new(&connection)
            .create(&CaseMeta {
                id: case.clone(),
                name: "Snapshot analysis result storage".into(),
                number: None,
                examiner: None,
                notes: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .unwrap();
        Self {
            root,
            connection,
            case,
        }
    }

    fn source(&self, id: &str) -> SourceRevisionRef {
        let bytes = format!("native source bytes for {id}");
        let path = self.root.path().join(format!("{id}.raw"));
        std::fs::write(&path, bytes.as_bytes()).unwrap();
        let source = DataSource {
            id: DataSourceId(id.into()),
            name: id.into(),
            kind: DataSourceKind::Raw,
            source_path: path,
            imported_at: Utc::now(),
            provenance: DataSourceProvenance::unknown(),
        };
        let mut storage = DataSourceStorage::source_db(id, None, None);
        storage.import_state = "ready_metadata".into();
        DataSourceRepo::new(&self.connection)
            .insert_with_storage(&self.case, &source, &storage)
            .unwrap();
        let control = open_source_db(self.root.path(), &source.id).unwrap();
        DataSourceRepo::new(&control)
            .upsert_source_local_metadata(&self.case, &source)
            .unwrap();
        let reference = SourceRevisionRepo::new(&self.connection)
            .bind(
                &self.case.0,
                id,
                &SourceRevisionContent::captured(
                    AcquisitionId(format!("acquisition:{id}")),
                    bytes.len() as u64,
                    None,
                    sha256(bytes.as_bytes()),
                ),
                true,
            )
            .unwrap();
        let store = persistence_source::detail::open_store_for_write(&control)
            .unwrap()
            .unwrap();
        store
            .write_envelope(
                KeyspaceName::Meta,
                b"snapshot-analysis-input".to_vec(),
                &VersionedEnvelope::new(
                    "snapshot_analysis_input",
                    1,
                    id,
                    id,
                    reference.revision,
                    bytes.into_bytes(),
                ),
                PersistMode::SyncAll,
            )
            .unwrap();
        seal_source_generation_with_control(
            &store,
            id,
            "input-attempt",
            &sha256(id.as_bytes()),
            &AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap();
        reference
    }

    fn receipt(
        &self,
        source: &SourceRevisionRef,
        output: &Value,
        attempt: &str,
    ) -> SnapshotAnalysisReceipt {
        let control =
            open_source_db(self.root.path(), &DataSourceId(source.source_id.clone())).unwrap();
        let store = persistence_source::detail::require_source_store(&control).unwrap();
        let manifest = read_source_generation_manifest(&store, &source.source_id).unwrap();
        SnapshotAnalysisReceipt::new(
            source.clone(),
            SourceRevisionRepo::new(&self.connection)
                .dependency_revisions(source)
                .unwrap(),
            "target-native-metadata".into(),
            BTreeSet::from(["metadata-capability@1".into()]),
            json!({
                "attempt": manifest.attempt_id,
                "immutable": manifest.immutable_data_digest,
                "projection": manifest.projection_batch_digest,
            })
            .to_string(),
            sha256(source.source_id.as_bytes()),
            sha256(&serde_json::to_vec(output).unwrap()),
            StructureAttemptRef {
                attempt_id: attempt.into(),
                owner: "analysis-owner".into(),
            },
            "native-metadata-analysis".into(),
            "snapshot-analysis-v1".into(),
        )
        .unwrap()
    }

    fn directory(&self, receipt: &SnapshotAnalysisReceipt) -> PathBuf {
        source_db_path(
            self.root.path(),
            &DataSourceId(receipt.source_revision.source_id.clone()),
        )
        .parent()
        .unwrap()
        .join("analysis-results")
        .join(&receipt.id)
    }
}

#[test]
fn result_round_trip_uses_a_separate_source_owned_native_generation() {
    let fixture = Fixture::new();
    let source = fixture.source("source-a");
    let output = json!({"objects": 7, "sequence": u64::MAX, "namespaces": ["metadata"]});
    let receipt = fixture.receipt(&source, &output, "analysis-a");
    let source_path = source_db_path(fixture.root.path(), &DataSourceId(source.source_id.clone()));
    let input_directory = source_path.parent().unwrap().join("source.fjall");
    let input_manifest = source_generation_manifest_digest(&input_directory).unwrap();
    let reference = write_analysis_result(fixture.root.path(), &receipt, &output).unwrap();
    assert_eq!(reference.source_id, source.source_id);
    assert_eq!(reference.receipt_id, receipt.id);
    assert_eq!(reference.output_sha256, receipt.output_sha256);
    assert_eq!(
        reference.generation_name,
        format!("analysis-results/{}", receipt.id)
    );
    assert_eq!(
        reference.manifest_sha256,
        source_generation_manifest_digest(fixture.directory(&receipt)).unwrap()
    );
    let saved = read_analysis_result(fixture.root.path(), &source, &receipt.id).unwrap();
    assert_eq!(saved.reference, reference);
    assert_eq!(saved.receipt, receipt);
    assert_eq!(saved.output, output);
    assert_eq!(saved.output["sequence"].as_u64(), Some(u64::MAX));
    assert_eq!(
        source_generation_manifest_digest(input_directory).unwrap(),
        input_manifest
    );
}

#[test]
fn seal_projection_digest_binds_actual_envelope_payload_bytes() {
    let fixture = Fixture::new();
    let source = fixture.source("source-a");
    let output = json!({"count": 2});
    let receipt = fixture.receipt(&source, &output, "analysis-a");
    write_analysis_result(fixture.root.path(), &receipt, &output).unwrap();
    let store = writable_result_store(&fixture.directory(&receipt));
    let envelope = stored_envelope(&store, &receipt);
    let manifest = read_source_generation_manifest(&store, &source.source_id).unwrap();
    assert_eq!(envelope.record_kind, "snapshot_analysis_result");
    assert_eq!(envelope.record_id, receipt.id);
    assert_eq!(envelope.source_id, source.source_id);
    assert_eq!(envelope.revision, source.revision);
    assert_eq!(manifest.source_id, source.source_id);
    assert_eq!(manifest.attempt_id, receipt.attempt.attempt_id);
    assert_eq!(manifest.projection_batch_digest, sha256(&envelope.payload));
    let payload: Value = serde_json::from_slice(&envelope.payload).unwrap();
    assert_eq!(payload["receipt"], serde_json::to_value(&receipt).unwrap());
    assert_eq!(payload["output"], output);
    assert_eq!(receipt.output_sha256, sha256(br#"{"count":2}"#));
}

#[test]
fn repeated_write_verifies_and_reuses_identical_contents_without_resealing() {
    let fixture = Fixture::new();
    let source = fixture.source("source-a");
    let output = json!({"count": 2});
    let receipt = fixture.receipt(&source, &output, "analysis-a");
    let first = write_analysis_result(fixture.root.path(), &receipt, &output).unwrap();
    let directory = fixture.directory(&receipt);
    let manifest_before = std::fs::read(directory.join(SOURCE_GENERATION_MANIFEST_FILE)).unwrap();
    let seal_before = std::fs::read(directory.join(SOURCE_GENERATION_SEAL_FILE)).unwrap();
    let second = write_analysis_result(fixture.root.path(), &receipt, &output).unwrap();
    assert_eq!(first, second);
    assert_eq!(
        std::fs::read(directory.join(SOURCE_GENERATION_MANIFEST_FILE)).unwrap(),
        manifest_before
    );
    assert_eq!(
        std::fs::read(directory.join(SOURCE_GENERATION_SEAL_FILE)).unwrap(),
        seal_before
    );
    assert!(read_analysis_result(fixture.root.path(), &source, &receipt.id).is_ok());
}

#[test]
fn changed_output_is_rejected_before_creating_or_overwriting_a_directory() {
    let fixture = Fixture::new();
    let source = fixture.source("source-a");
    let output = json!({"count": 2});
    let receipt = fixture.receipt(&source, &output, "analysis-a");
    let wrong = json!({"count": 3});
    assert!(matches!(
        write_analysis_result(fixture.root.path(), &receipt, &wrong),
        Err(AnalysisResultError::OutputDigestMismatch)
    ));
    assert!(!fixture.directory(&receipt).exists());
    write_analysis_result(fixture.root.path(), &receipt, &output).unwrap();
    assert!(write_analysis_result(fixture.root.path(), &receipt, &wrong).is_err());
    assert_eq!(
        read_analysis_result(fixture.root.path(), &source, &receipt.id)
            .unwrap()
            .output,
        output
    );
}

#[test]
fn separate_attempts_produce_independent_immutable_directories() {
    let fixture = Fixture::new();
    let source = fixture.source("source-a");
    let output = json!({"count": 2});
    let first = fixture.receipt(&source, &output, "analysis-a");
    let second = fixture.receipt(&source, &output, "analysis-b");
    assert_ne!(first.id, second.id);
    let first_reference = write_analysis_result(fixture.root.path(), &first, &output).unwrap();
    let second_reference = write_analysis_result(fixture.root.path(), &second, &output).unwrap();
    assert_ne!(
        first_reference.generation_name,
        second_reference.generation_name
    );
    assert_eq!(
        read_analysis_result(fixture.root.path(), &source, &first.id)
            .unwrap()
            .reference,
        first_reference
    );
    assert_eq!(
        read_analysis_result(fixture.root.path(), &source, &second.id)
            .unwrap()
            .reference,
        second_reference
    );
}

#[test]
fn source_and_case_boundaries_are_checked_before_any_result_write() {
    let fixture = Fixture::new();
    let source = fixture.source("source-a");
    let output = json!({"count": 2});
    let receipt = fixture.receipt(&source, &output, "analysis-a");
    let mut foreign_source = source.clone();
    foreign_source.case_id = "another-case".into();
    let foreign = rebuild_receipt(&receipt, foreign_source.clone(), vec![foreign_source]);
    assert!(matches!(
        write_analysis_result(fixture.root.path(), &foreign, &output),
        Err(AnalysisResultError::SourceOwnerMismatch)
    ));
    assert!(!fixture.directory(&foreign).exists());
    let another = Fixture::new();
    another.source("source-a");
    assert!(write_analysis_result(another.root.path(), &receipt, &output).is_err());
}

#[test]
fn source_local_control_owner_must_match_the_registered_source() {
    let fixture = Fixture::new();
    let source = fixture.source("source-a");
    let output = json!({"count": 2});
    let receipt = fixture.receipt(&source, &output, "analysis-a");
    let control =
        open_source_db(fixture.root.path(), &DataSourceId(source.source_id.clone())).unwrap();
    control
        .execute(
            "UPDATE source_meta SET value='different-source' WHERE key='data_source_id'",
            [],
        )
        .unwrap();
    assert!(matches!(
        write_analysis_result(fixture.root.path(), &receipt, &output),
        Err(AnalysisResultError::SourceOwnerMismatch)
    ));
    assert!(!fixture.directory(&receipt).exists());
}

#[test]
fn existing_receipt_cannot_be_read_under_another_source() {
    let fixture = Fixture::new();
    let first = fixture.source("source-a");
    let second = fixture.source("source-b");
    let output = json!({"count": 2});
    let receipt = fixture.receipt(&first, &output, "analysis-a");
    write_analysis_result(fixture.root.path(), &receipt, &output).unwrap();
    assert!(read_analysis_result(fixture.root.path(), &second, &receipt.id).is_err());
    let wrong_directory =
        source_db_path(fixture.root.path(), &DataSourceId(second.source_id.clone()))
            .parent()
            .unwrap()
            .join("analysis-results");
    assert!(!wrong_directory.exists());
}

#[test]
fn stale_source_revisions_reject_reads_and_writes() {
    let fixture = Fixture::new();
    let source = fixture.source("source-a");
    let output = json!({"count": 2});
    let receipt = fixture.receipt(&source, &output, "analysis-a");
    write_analysis_result(fixture.root.path(), &receipt, &output).unwrap();
    let next = SourceRevisionRepo::new(&fixture.connection)
        .bind(
            &fixture.case.0,
            &source.source_id,
            &SourceRevisionContent::captured(
                AcquisitionId("replacement-acquisition".into()),
                20,
                None,
                sha256(b"replacement content"),
            ),
            true,
        )
        .unwrap();
    assert_ne!(source.revision, next.revision);
    assert!(write_analysis_result(fixture.root.path(), &receipt, &output).is_err());
    assert!(read_analysis_result(fixture.root.path(), &source, &receipt.id).is_err());
    assert!(fixture.directory(&receipt).is_dir());
}

#[test]
fn result_requires_the_complete_current_dependency_set() {
    let fixture = Fixture::new();
    let parent = fixture.source("parent-a");
    let source = fixture.source("source-a");
    SourceRevisionRepo::new(&fixture.connection)
        .bind_inputs(
            &fixture.case.0,
            &source.source_id,
            std::slice::from_ref(&parent),
        )
        .unwrap();
    let output = json!({"count": 2});
    let receipt = fixture.receipt(&source, &output, "analysis-a");
    assert_eq!(receipt.dependency_revisions.len(), 2);
    let incomplete = rebuild_receipt(&receipt, source.clone(), vec![source.clone()]);
    assert!(matches!(
        write_analysis_result(fixture.root.path(), &incomplete, &output),
        Err(AnalysisResultError::DependencyMismatch)
    ));
    assert!(!fixture.directory(&incomplete).exists());
    write_analysis_result(fixture.root.path(), &receipt, &output).unwrap();
    let loaded = read_analysis_result(fixture.root.path(), &source, &receipt.id).unwrap();
    assert_eq!(
        loaded.receipt.dependency_revisions,
        receipt.dependency_revisions
    );
}

#[test]
fn receipt_path_validation_rejects_traversal_alternate_prefixes_and_nonhex_names() {
    let fixture = Fixture::new();
    let source = fixture.source("source-a");
    for id in [
        "../outside".to_string(),
        "snapshot-analysis-receipt-v1-../../outside".into(),
        format!("snapshot-analysis-receipt-v1-{}", "A".repeat(64)),
        format!("snapshot-analysis-receipt-v1-{}", "g".repeat(64)),
        format!("snapshot-analysis-receipt-v2-{}", "a".repeat(64)),
        format!("snapshot-analysis-receipt-v1-{}:stream", "a".repeat(64)),
        format!("snapshot-analysis-receipt-v1-{}", "a".repeat(63)),
    ] {
        assert!(matches!(
            read_analysis_result(fixture.root.path(), &source, &id),
            Err(AnalysisResultError::InvalidReceiptId)
        ));
    }
}

#[test]
fn existing_unsealed_generation_is_rejected_without_repair() {
    let fixture = Fixture::new();
    let source = fixture.source("source-a");
    let output = json!({"count": 2});
    let receipt = fixture.receipt(&source, &output, "analysis-a");
    let directory = fixture.directory(&receipt);
    std::fs::create_dir_all(&directory).unwrap();
    {
        let store = writable_result_store(&directory);
        store
            .write_raw(
                KeyspaceName::Meta,
                b"incomplete".to_vec(),
                b"present".to_vec(),
                PersistMode::SyncAll,
            )
            .unwrap();
    }
    assert!(write_analysis_result(fixture.root.path(), &receipt, &output).is_err());
    assert!(!directory.join(SOURCE_GENERATION_MANIFEST_FILE).exists());
    assert!(!directory.join(SOURCE_GENERATION_SEAL_FILE).exists());
}

#[test]
fn missing_generation_or_seal_is_rejected_and_never_recreated_by_reads() {
    let fixture = Fixture::new();
    let source = fixture.source("source-a");
    let output = json!({"count": 2});
    let receipt = fixture.receipt(&source, &output, "analysis-a");
    assert!(read_analysis_result(fixture.root.path(), &source, &receipt.id).is_err());
    assert!(!fixture.directory(&receipt).exists());
    write_analysis_result(fixture.root.path(), &receipt, &output).unwrap();
    let seal = fixture
        .directory(&receipt)
        .join(SOURCE_GENERATION_SEAL_FILE);
    std::fs::remove_file(&seal).unwrap();
    assert!(read_analysis_result(fixture.root.path(), &source, &receipt.id).is_err());
    assert!(write_analysis_result(fixture.root.path(), &receipt, &output).is_err());
    assert!(!seal.exists());
}

#[test]
fn valid_envelope_with_changed_output_is_rejected_by_its_receipt_digest() {
    let fixture = Fixture::new();
    let source = fixture.source("source-a");
    let output = json!({"count": 2});
    let receipt = fixture.receipt(&source, &output, "analysis-a");
    write_analysis_result(fixture.root.path(), &receipt, &output).unwrap();
    {
        let store = writable_result_store(&fixture.directory(&receipt));
        let mut envelope = stored_envelope(&store, &receipt);
        let mut payload: Value = serde_json::from_slice(&envelope.payload).unwrap();
        payload["output"]["count"] = json!(3);
        envelope = with_payload(envelope, serde_json::to_vec(&payload).unwrap());
        store
            .write_envelope(
                KeyspaceName::Meta,
                result_key(&receipt),
                &envelope,
                PersistMode::SyncAll,
            )
            .unwrap();
    }
    assert!(matches!(
        read_analysis_result(fixture.root.path(), &source, &receipt.id),
        Err(AnalysisResultError::OutputDigestMismatch)
    ));
    assert!(write_analysis_result(fixture.root.path(), &receipt, &output).is_err());
}

#[test]
fn manifest_payload_digest_detects_semantically_equal_json_with_different_bytes() {
    let fixture = Fixture::new();
    let source = fixture.source("source-a");
    let output = json!({"count": 2});
    let receipt = fixture.receipt(&source, &output, "analysis-a");
    write_analysis_result(fixture.root.path(), &receipt, &output).unwrap();
    {
        let store = writable_result_store(&fixture.directory(&receipt));
        let envelope = stored_envelope(&store, &receipt);
        let mut payload = envelope.payload.clone();
        payload.push(b'\n');
        let changed = with_payload(envelope, payload);
        store
            .write_envelope(
                KeyspaceName::Meta,
                result_key(&receipt),
                &changed,
                PersistMode::SyncAll,
            )
            .unwrap();
    }
    assert!(matches!(
        read_analysis_result(fixture.root.path(), &source, &receipt.id),
        Err(AnalysisResultError::ManifestMismatch)
    ));
}

#[test]
fn envelope_owner_and_identity_are_verified_separately_from_payload() {
    for field in ["source", "record", "schema", "revision", "kind"] {
        let fixture = Fixture::new();
        let source = fixture.source("source-a");
        let output = json!({"count": 2});
        let receipt = fixture.receipt(&source, &output, "analysis-a");
        write_analysis_result(fixture.root.path(), &receipt, &output).unwrap();
        {
            let store = writable_result_store(&fixture.directory(&receipt));
            let mut envelope = stored_envelope(&store, &receipt);
            match field {
                "source" => envelope.source_id = "different-source".into(),
                "record" => envelope.record_id = "different-record".into(),
                "schema" => envelope.schema_version = 2,
                "revision" => envelope.revision += 1,
                "kind" => envelope.record_kind = "different-kind".into(),
                _ => unreachable!(),
            }
            store
                .write_envelope(
                    KeyspaceName::Meta,
                    result_key(&receipt),
                    &envelope,
                    PersistMode::SyncAll,
                )
                .unwrap();
        }
        assert!(matches!(
            read_analysis_result(fixture.root.path(), &source, &receipt.id),
            Err(AnalysisResultError::EnvelopeMismatch)
        ));
    }
}

#[test]
fn corrupted_manifest_and_receipt_identity_are_rejected() {
    let fixture = Fixture::new();
    let source = fixture.source("source-a");
    let output = json!({"count": 2});
    let receipt = fixture.receipt(&source, &output, "analysis-a");
    write_analysis_result(fixture.root.path(), &receipt, &output).unwrap();
    let manifest = fixture
        .directory(&receipt)
        .join(SOURCE_GENERATION_MANIFEST_FILE);
    std::fs::write(&manifest, b"{}").unwrap();
    assert!(read_analysis_result(fixture.root.path(), &source, &receipt.id).is_err());
    let mut invalid = receipt;
    invalid.id = format!("snapshot-analysis-receipt-v1-{}", "a".repeat(64));
    assert!(matches!(
        write_analysis_result(fixture.root.path(), &invalid, &output),
        Err(AnalysisResultError::Receipt(_))
    ));
}

#[test]
fn pure_result_references_round_trip_without_handles_or_extra_fields() {
    let fixture = Fixture::new();
    let source = fixture.source("source-a");
    let output = json!({"count": 2});
    let receipt = fixture.receipt(&source, &output, "analysis-a");
    let reference = write_analysis_result(fixture.root.path(), &receipt, &output).unwrap();
    let encoded = serde_json::to_string(&reference).unwrap();
    assert_eq!(
        serde_json::from_str::<AnalysisResultRef>(&encoded).unwrap(),
        reference
    );
    let mut value = serde_json::to_value(reference).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 5);
    value["unknownField"] = json!(true);
    assert!(serde_json::from_value::<AnalysisResultRef>(value).is_err());
}

#[test]
fn returned_results_release_the_real_source_connection_pin() {
    let fixture = Fixture::new();
    let source = fixture.source("source-a");
    let output = json!({"count": 2});
    let receipt = fixture.receipt(&source, &output, "analysis-a");
    let reference = write_analysis_result(fixture.root.path(), &receipt, &output).unwrap();
    let result = read_analysis_result(fixture.root.path(), &source, &receipt.id).unwrap();
    let source_directory =
        source_db_path(fixture.root.path(), &DataSourceId(source.source_id.clone()))
            .parent()
            .unwrap()
            .to_path_buf();
    let guard =
        persistence_source::control::connections::try_begin_generation_gc(&source_directory)
            .unwrap();
    assert!(guard.is_some());
    assert_eq!(reference, result.reference);
}

#[test]
fn receipt_generation_identity_must_match_the_current_real_input_manifest() {
    let fixture = Fixture::new();
    let source = fixture.source("source-a");
    let output = json!({"count": 2});
    let receipt = fixture.receipt(&source, &output, "analysis-a");
    write_analysis_result(fixture.root.path(), &receipt, &output).unwrap();
    let control =
        open_source_db(fixture.root.path(), &DataSourceId(source.source_id.clone())).unwrap();
    let next_name = "source.fjall.build.analysis-next";
    let next = source_db_path(fixture.root.path(), &DataSourceId(source.source_id.clone()))
        .parent()
        .unwrap()
        .join(next_name);
    let store = writable_result_store(&next);
    store
        .write_envelope(
            KeyspaceName::Meta,
            b"updated-input".to_vec(),
            &VersionedEnvelope::new(
                "snapshot_analysis_input",
                1,
                "updated-input",
                &source.source_id,
                source.revision,
                b"updated immutable metadata generation".to_vec(),
            ),
            PersistMode::SyncAll,
        )
        .unwrap();
    seal_source_generation_with_control(
        &store,
        &source.source_id,
        "input-generation-next",
        &sha256(b"updated immutable metadata generation"),
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    persistence_source::detail::bind_generation(&control, next_name).unwrap();
    assert!(matches!(
        write_analysis_result(fixture.root.path(), &receipt, &output),
        Err(AnalysisResultError::GenerationMismatch)
    ));
    assert!(matches!(
        read_analysis_result(fixture.root.path(), &source, &receipt.id),
        Err(AnalysisResultError::GenerationMismatch)
    ));
}

#[test]
fn mathematically_valid_receipt_with_an_unbound_generation_cannot_create_a_result() {
    let fixture = Fixture::new();
    let source = fixture.source("source-a");
    let output = json!({"count": 2});
    let receipt = fixture.receipt(&source, &output, "analysis-a");
    let mut changed = receipt.clone();
    changed.generation_identity = "unbound-generation".into();
    let changed = rebuild_receipt(
        &changed,
        changed.source_revision.clone(),
        changed.dependency_revisions.clone(),
    );
    changed.validate().unwrap();
    assert!(matches!(
        write_analysis_result(fixture.root.path(), &changed, &output),
        Err(AnalysisResultError::GenerationMismatch)
    ));
    assert!(!fixture.directory(&changed).exists());
}

#[test]
fn readonly_result_open_does_not_create_missing_runtime_artifact_directories() {
    let fixture = Fixture::new();
    let source = fixture.source("source-a");
    let output = json!({"count": 2});
    let receipt = fixture.receipt(&source, &output, "analysis-a");
    write_analysis_result(fixture.root.path(), &receipt, &output).unwrap();
    let objects = fixture.directory(&receipt).join("artifacts/objects");
    assert!(objects.is_dir());
    std::fs::remove_dir(&objects).unwrap();
    assert!(matches!(
        read_analysis_result(fixture.root.path(), &source, &receipt.id),
        Err(AnalysisResultError::Runtime(
            persistence_source::runtime::StorageRuntimeError::Artifacts(_)
        ))
    ));
    assert!(!objects.exists());
}

#[test]
fn missing_source_manifest_refuses_results_before_creating_result_directories() {
    let fixture = Fixture::new();
    let source = fixture.source("source-a");
    let output = json!({"count": 2});
    let receipt = fixture.receipt(&source, &output, "analysis-a");
    let input = source_db_path(fixture.root.path(), &DataSourceId(source.source_id.clone()))
        .parent()
        .unwrap()
        .join("source.fjall");
    std::fs::remove_file(input.join(SOURCE_GENERATION_SEAL_FILE)).unwrap();
    assert!(write_analysis_result(fixture.root.path(), &receipt, &output).is_err());
    assert!(!fixture.directory(&receipt).exists());
}

#[test]
fn a_file_cannot_be_used_as_the_analysis_result_parent_directory() {
    let fixture = Fixture::new();
    let source = fixture.source("source-a");
    let output = json!({"count": 2});
    let receipt = fixture.receipt(&source, &output, "analysis-a");
    let directory = fixture.directory(&receipt);
    let parent = directory.parent().unwrap();
    std::fs::write(parent, b"existing file must be preserved").unwrap();
    assert!(matches!(
        write_analysis_result(fixture.root.path(), &receipt, &output),
        Err(AnalysisResultError::UnsafePath)
    ));
    assert_eq!(
        std::fs::read(parent).unwrap(),
        b"existing file must be preserved"
    );
}

#[cfg(unix)]
#[test]
fn analysis_result_parent_links_cannot_redirect_writes_to_another_source() {
    let fixture = Fixture::new();
    let source = fixture.source("source-a");
    let other = fixture.source("source-b");
    let output = json!({"count": 2});
    let receipt = fixture.receipt(&source, &output, "analysis-a");
    let directory = fixture.directory(&receipt);
    let other_directory =
        source_db_path(fixture.root.path(), &DataSourceId(other.source_id.clone()))
            .parent()
            .unwrap()
            .to_path_buf();
    std::os::unix::fs::symlink(&other_directory, directory.parent().unwrap()).unwrap();
    assert!(matches!(
        write_analysis_result(fixture.root.path(), &receipt, &output),
        Err(AnalysisResultError::UnsafePath)
    ));
    assert!(!other_directory.join(&receipt.id).exists());
}

fn result_key(receipt: &SnapshotAnalysisReceipt) -> Vec<u8> {
    encode_key(
        "snapshot_analysis_result",
        &[KeyPart::Text(receipt.id.clone())],
    )
    .unwrap()
}

fn writable_result_store(path: &Path) -> SourceStore {
    persistence_source::runtime::ReadWriteStorageRuntime::open(path)
        .unwrap()
        .source()
        .source_store()
        .clone()
}

fn stored_envelope(store: &SourceStore, receipt: &SnapshotAnalysisReceipt) -> VersionedEnvelope {
    store
        .read_envelope(
            &store.database().read_snapshot(),
            KeyspaceName::Meta,
            &result_key(receipt),
        )
        .unwrap()
        .unwrap()
}

fn with_payload(envelope: VersionedEnvelope, payload: Vec<u8>) -> VersionedEnvelope {
    VersionedEnvelope::new(
        envelope.record_kind,
        envelope.schema_version,
        envelope.record_id,
        envelope.source_id,
        envelope.revision,
        payload,
    )
}

fn rebuild_receipt(
    receipt: &SnapshotAnalysisReceipt,
    source: SourceRevisionRef,
    dependencies: Vec<SourceRevisionRef>,
) -> SnapshotAnalysisReceipt {
    SnapshotAnalysisReceipt::new(
        source,
        dependencies,
        receipt.target_id.clone(),
        receipt.input_capability_ids.clone(),
        receipt.generation_identity.clone(),
        receipt.scope_fingerprint.clone(),
        receipt.output_sha256.clone(),
        receipt.attempt.clone(),
        receipt.verifier_id.clone(),
        receipt.policy_version.clone(),
    )
    .unwrap()
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
