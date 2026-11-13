use super::*;
use crate::analysis_service::extraction::{
    reader::CandidateSource, run_analysis_extraction_with_source_and_progress,
};
use domain::{
    CaseId, DataSource, DataSourceId, DataSourceKind, DataSourcePlatform, DataSourceProvenance,
};
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::tests::registry_hive as hive_fixture;

struct Fixture {
    connection: rusqlite::Connection,
    evidence: PathBuf,
    _root: tempfile::TempDir,
}

impl Fixture {
    fn new(hives: &[&str], log_size: usize) -> Self {
        Self::build(hives, log_size, false)
    }

    fn build(hives: &[&str], log_size: usize, unknown_log_size: bool) -> Self {
        let scratch =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../output/repair-2026-11-13/registry");
        fs::create_dir_all(&scratch).unwrap();
        let root = tempfile::tempdir_in(scratch).unwrap();
        let evidence = root.path().join("evidence");
        let bytes = hive_fixture::hive_bytes();
        for path in hives {
            let path = evidence.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, &bytes).unwrap();
            for suffix in ["LOG1", "LOG2"] {
                fs::write(
                    path.with_file_name(format!(
                        "{}.{}",
                        path.file_name().unwrap().to_string_lossy(),
                        suffix
                    )),
                    vec![1; log_size],
                )
                .unwrap();
            }
        }
        let source = DataSource {
            id: DataSourceId("registry-source".into()),
            name: "Registry source".into(),
            kind: DataSourceKind::LogicalDirectory,
            source_path: evidence.clone(),
            imported_at: chrono::Utc::now(),
            provenance: DataSourceProvenance::unknown(),
        };
        let connection = persistence_source::control::opening::open_source_database(
            &root.path().join("case/sources/registry-source/source.db"),
            &source.id.0,
        )
        .unwrap();
        DataSourceRepo::new(&connection)
            .upsert_source_local_metadata(&CaseId("case".into()), &source)
            .unwrap();
        let reader = evidence_core::LogicalFsReader::open(&evidence, "Registry source").unwrap();
        evidence_access::file_service::enumerate_filesystem(&connection, &source.id, &reader)
            .unwrap();
        if unknown_log_size {
            let entry = FileRepo::new(&connection)
                .find_by_data_source(&source.id)
                .unwrap()
                .into_iter()
                .find(|entry| entry.path == format!("{SOFTWARE}.LOG1"))
                .unwrap();
            let store = persistence_source::detail::require_source_store(&connection).unwrap();
            let catalog =
                persistence_source::fjall::CatalogRecordStore::from_source_store((*store).clone());
            let mut log = catalog
                .get(&catalog.read_snapshot(), &source.id.0, &entry.id.0)
                .unwrap()
                .unwrap();
            let partition = log.partition_index;
            log.size = None;
            catalog
                .upsert(&log, persistence_source::fjall::PersistMode::SyncAll)
                .unwrap();
            let stored = catalog
                .get(&catalog.read_snapshot(), &source.id.0, &entry.id.0)
                .unwrap()
                .unwrap();
            assert_eq!(stored, log);
            assert_eq!(stored.partition_index, partition);
        }
        seal(&connection, &source.id.0);
        Self {
            connection,
            evidence,
            _root: root,
        }
    }

    fn candidates(&self) -> Vec<EvidenceCandidate> {
        crate::analysis_service::candidates::evidence_candidates_for_categories(
            &self.connection,
            &["Registry"],
        )
        .unwrap()
    }

    fn run(
        &self,
        reads: &mut Vec<String>,
    ) -> crate::analysis_service::extraction::AnalysisExtractionExecution {
        run_analysis_extraction_with_source_and_progress(
            &self.connection,
            "case",
            DataSourcePlatform::Windows,
            &["Registry"],
            &AtomicBool::new(false),
            None,
            &mut |_| {},
            |candidate, _| {
                reads.push(candidate.path.clone());
                fs::File::open(self.evidence.join(&candidate.path))
                    .map(|file| CandidateSource::Reader(Box::new(file)))
            },
        )
        .unwrap()
    }
}

fn seal(connection: &rusqlite::Connection, source_id: &str) {
    use persistence_source::fjall::{KeyPart, KeyspaceName, PersistMode, VersionedEnvelope};
    let store = persistence_source::detail::open_store_for_write(connection)
        .unwrap()
        .unwrap();
    let key =
        persistence_source::fjall::encode_key("projection", &[KeyPart::Text(source_id.into())])
            .unwrap();
    let marker = VersionedEnvelope::new(
        "source_projection_complete",
        1,
        format!("projection:{source_id}"),
        source_id,
        1,
        source_id.as_bytes().to_vec(),
    );
    let receipt = store
        .write_envelope(KeyspaceName::Meta, key, &marker, PersistMode::SyncAll)
        .unwrap();
    persistence_source::fjall::seal_source_generation_with_control(
        &store,
        source_id,
        "fixture",
        &hex::encode(receipt.batch_digest),
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
}

const SOFTWARE: &str = "Windows/System32/config/SOFTWARE";

#[test]
fn physical_log_read_failures_count_each_pending_candidate_once() {
    for hives in [
        vec![SOFTWARE],
        vec![
            SOFTWARE,
            "Windows/System32/config/SYSTEM",
            "Users/test/NTUSER.DAT",
        ],
    ] {
        let fixture = Fixture::new(&hives, 8);
        assert_eq!(fixture.candidates().len(), hives.len());
        for path in &hives {
            fs::remove_file(fixture.evidence.join(format!("{path}.LOG1"))).unwrap();
        }
        let execution = fixture.run(&mut Vec::new());
        assert_eq!(execution.retryable_failure_count, hives.len() as u64);
        assert_eq!(execution.dto.warnings.len(), hives.len());
        for path in hives {
            assert_eq!(
                execution
                    .dto
                    .warnings
                    .iter()
                    .filter(|warning| warning.starts_with(&format!("{path} LOG1 read failed:")))
                    .count(),
                1
            );
        }
        let checkpoints: u64 = fixture
            .connection
            .query_row(
                "SELECT COUNT(*) FROM source_meta WHERE key GLOB 'analysis_candidate_scan:*'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(checkpoints, 0);
    }
}

#[test]
fn physical_primary_read_failure_keeps_one_original_reason() {
    let fixture = Fixture::new(&[SOFTWARE], 0);
    fs::remove_file(fixture.evidence.join(SOFTWARE)).unwrap();
    let execution = fixture.run(&mut Vec::new());
    assert_eq!(execution.retryable_failure_count, 1);
    assert_eq!(execution.dto.warnings.len(), 1);
    assert!(execution.dto.warnings[0].starts_with(&format!("{SOFTWARE} read failed:")));
}

#[test]
fn zero_length_registry_logs_read_eof_and_preserve_hive_parsing() {
    let fixture = Fixture::new(&[SOFTWARE], 0);
    let mut reads = Vec::new();
    let execution = fixture.run(&mut reads);
    assert_eq!(execution.retryable_failure_count, 0);
    assert_eq!(
        reads,
        [
            SOFTWARE.to_owned(),
            format!("{SOFTWARE}.LOG1"),
            format!("{SOFTWARE}.LOG2")
        ]
    );
    assert_eq!(execution.dto.scanned_count, 1);
}

#[test]
fn unknown_catalog_size_nonempty_log_is_read_and_rejected() {
    let fixture = Fixture::build(&[SOFTWARE], 8, true);
    let log = FileRepo::new(&fixture.connection)
        .find_by_data_source(&DataSourceId("registry-source".into()))
        .unwrap()
        .into_iter()
        .find(|entry| entry.path == format!("{SOFTWARE}.LOG1"))
        .unwrap();
    assert_eq!(log.size, None);
    let candidates = fixture.candidates();
    let candidate = candidates
        .iter()
        .find(|candidate| candidate.path == SOFTWARE)
        .unwrap();
    assert!(candidate
        .companions
        .iter()
        .any(|companion| companion.file_id == log.id));
    assert_eq!(
        fs::metadata(fixture.evidence.join(&log.path))
            .unwrap()
            .len(),
        8
    );
    let mut reads = Vec::new();
    let execution = fixture.run(&mut reads);
    assert!(reads.contains(&log.path));
    assert_eq!(execution.retryable_failure_count, 1);
    assert_eq!(execution.dto.warnings.len(), 1);
    assert_eq!(
        execution.dto.warnings[0],
        format!("{SOFTWARE} LOG1 read failed: registry log was truncated")
    );
}

#[test]
fn non_pending_dependency_failures_remain_separate_from_pending_results() {
    let fixture = Fixture::new(
        &[
            "Windows/System32/config/SYSTEM",
            "Windows/System32/config/SAM",
        ],
        0,
    );
    fs::remove_file(fixture.evidence.join("Windows/System32/config/SYSTEM")).unwrap();
    let candidates = fixture.candidates();
    let by_id = FileRepo::new(&fixture.connection)
        .find_by_data_source(&DataSourceId("registry-source".into()))
        .unwrap()
        .into_iter()
        .map(|entry| (entry.id, entry.path))
        .collect::<HashMap<_, _>>();
    let context = preload_registry_context(
        &candidates,
        &AtomicBool::new(false),
        |id, _| {
            fs::File::open(fixture.evidence.join(&by_id[id]))
                .map(|file| Box::new(file) as Box<dyn Read>)
        },
        |candidate| Ok(candidate.path.ends_with("/SYSTEM")),
    )
    .unwrap();
    assert_eq!(context.dependency_warnings.len(), 1);
    assert!(
        context.dependency_warnings[0].starts_with("Windows/System32/config/SYSTEM read failed:")
    );
    let system = candidates
        .iter()
        .find(|candidate| candidate.path.ends_with("/SYSTEM"))
        .unwrap();
    assert!(context.failure_reason(system).is_none());
    let sam = candidates
        .iter()
        .find(|candidate| candidate.path.ends_with("/SAM"))
        .unwrap();
    assert!(context.snapshot(sam).is_some());
}
