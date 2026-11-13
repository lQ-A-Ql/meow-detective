use app_services::analysis_service::{
    run_analysis_extraction_execution_with_cancel, AnalysisExtractionExecution,
    AnalysisServiceError,
};
use artifacts_core::{ArtifactContext, ArtifactExtractor, VecSink};
use chrono::Utc;
use domain::{
    CaseId, DataSource, DataSourceId, DataSourceKind, DataSourcePlatform, DataSourceProvenance,
    EntryType, FileEntry, FileEntryId,
};
use persistence_source::repositories::{
    artifact_repo::ArtifactRepo, file_repo::FileRepo, timeline_repo::TimelineRepo,
};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use rusqlite::Connection;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

struct NativeSource {
    connection: Connection,
    input: PathBuf,
    entry: FileEntry,
    _directory: tempfile::TempDir,
}

impl NativeSource {
    fn new() -> Self {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.codex-tmp/work");
        fs::create_dir_all(&workspace).expect("create test workspace");
        let directory = tempfile::tempdir_in(workspace).expect("create test case");
        let root = directory.path();
        let source_id = DataSourceId("native-source".into());
        let connection =
            app_services::source_db::open_source_db(root, &source_id).expect("open source");
        let input = root.join("notepad.lnk");
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../testdata/fixtures/public-small/windows-parsers/notepad-xp.lnk"),
            &input,
        )
        .expect("copy published Windows link fixture");
        DataSourceRepo::new(&connection)
            .upsert_source_local_metadata(
                &CaseId("native-case".into()),
                &DataSource {
                    id: source_id.clone(),
                    name: "Native Windows fixture".into(),
                    kind: DataSourceKind::LogicalDirectory,
                    source_path: root.to_path_buf(),
                    imported_at: Utc::now(),
                    provenance: DataSourceProvenance::unknown(),
                },
            )
            .expect("register source");
        let entry = FileEntry {
            id: FileEntryId("native-link".into()),
            parent_id: None,
            data_source_id: source_id,
            path: "Users/alice/AppData/Roaming/Microsoft/Windows/Recent/notepad.lnk".into(),
            name: "notepad.lnk".into(),
            entry_type: EntryType::File,
            size: Some(fs::metadata(&input).expect("input metadata").len()),
            ext: Some("lnk".into()),
            deleted: false,
            hidden: false,
            system: false,
            encrypted: false,
            read_only: false,
            archive: false,
            unix_mode: None,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        };
        FileRepo::new(&connection)
            .insert_batch(std::slice::from_ref(&entry))
            .expect("insert file");
        let source = Self {
            connection,
            input,
            entry,
            _directory: directory,
        };
        source.seal();
        source
    }

    fn seal(&self) {
        let store = persistence_source::detail::open_store_for_write(&self.connection)
            .expect("open native store")
            .expect("native store exists");
        persistence_source::fjall::replace_source_generation(
            &store,
            &self.entry.data_source_id.0,
            "native-parser-fixture",
            &"a".repeat(64),
        )
        .expect("seal native store");
    }

    fn run(
        &self,
        cancel: &AtomicBool,
    ) -> Result<AnalysisExtractionExecution, AnalysisServiceError> {
        run_analysis_extraction_execution_with_cancel(
            &self.connection,
            "native-case",
            DataSourcePlatform::Windows,
            &["UserActivity"],
            cancel,
            |id, _limit| {
                assert_eq!(id, &self.entry.id);
                File::open(&self.input).map(|file| Box::new(file) as Box<dyn Read>)
            },
        )
    }

    fn artifact_ids(&self) -> Vec<String> {
        let mut ids = ArtifactRepo::new(&self.connection)
            .list_by_family(Some("LNK"))
            .expect("read links")
            .into_iter()
            .map(|artifact| artifact.id.0)
            .collect::<Vec<_>>();
        ids.sort();
        ids
    }
}

#[test]
fn published_link_produces_attributed_artifact_and_canonical_timeline() {
    let source = NativeSource::new();
    let execution = source.run(&AtomicBool::new(false)).expect("parse link");
    assert_eq!(execution.dto.artifact_count, 1);
    assert_eq!(execution.retryable_failure_count, 0);
    assert!(execution.dto.warnings.is_empty());
    let artifacts = ArtifactRepo::new(&source.connection)
        .list_by_family(Some("LNK"))
        .unwrap();
    assert_eq!(
        artifacts[0].source_object_id.as_ref(),
        Some(&source.entry.id)
    );
    assert_eq!(
        artifacts[0].extractor_id.as_deref(),
        Some("windows.native.activity.lnk")
    );
    assert!(artifacts[0].attrs["target_path"]
        .as_str()
        .unwrap()
        .ends_with("notepad.exe"));
    let events = TimelineRepo::new(&source.connection)
        .query_filtered(0, 20, None, None, None)
        .unwrap();
    assert_eq!(events.len(), 3);
    assert!(events.iter().all(|event| {
        event.source_object_id == source.entry.id.0
            && event.parser_id.as_deref() == Some("windows.native.activity.lnk")
            && matches!(
                event.event_type.as_str(),
                "FILE_CREATED" | "FILE_MODIFIED" | "FILE_ACCESSED"
            )
    }));
    let original_ids = source.artifact_ids();
    let repeated = source.run(&AtomicBool::new(false)).expect("repeat parse");
    assert_eq!(repeated.dto.checkpoint_hit_count, 1);
    assert_eq!(source.artifact_ids(), original_ids);
}

#[test]
fn successful_run_replaces_only_unattributed_legacy_output_for_that_file() {
    let source = NativeSource::new();
    let mut sink = VecSink::new();
    artifacts_windows::LnkExtractor
        .run(
            ArtifactContext {
                file_id: source.entry.id.clone(),
                file_path: source.entry.path.clone(),
                reader: Box::new(File::open(&source.input).unwrap()),
            },
            &mut sink,
        )
        .expect("parse published link");
    assert_eq!(sink.artifacts.len(), 1);
    let legacy_id = sink.artifacts[0].id.0.clone();
    ArtifactRepo::new(&source.connection)
        .insert_batch(
            &sink.artifacts,
            "native-case",
            &source.entry.data_source_id.0,
        )
        .expect("write legacy output");
    source.seal();
    source
        .run(&AtomicBool::new(false))
        .expect("replace legacy output");
    let ids = source.artifact_ids();
    assert_eq!(ids.len(), 1);
    assert_ne!(ids[0], legacy_id);
}

#[test]
fn cancelled_run_does_not_publish_native_output() {
    let source = NativeSource::new();
    assert!(matches!(
        source.run(&AtomicBool::new(true)),
        Err(AnalysisServiceError::Cancelled)
    ));
    assert!(source.artifact_ids().is_empty());
}

#[test]
#[cfg(windows)]
fn sharing_failure_preserves_output_and_successful_retry_replaces_it() {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;

    let mut source = NativeSource::new();
    source.run(&AtomicBool::new(false)).expect("first run");
    let previous = source.artifact_ids();
    assert_eq!(previous.len(), 1);
    let modified = std::time::SystemTime::now();
    File::options()
        .write(true)
        .open(&source.input)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    source.entry.modified_at = Some(modified.into());
    FileRepo::new(&source.connection)
        .insert_batch(std::slice::from_ref(&source.entry))
        .expect("update input identity");
    source.seal();

    let locked = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&source.input)
        .expect("hold exclusive input handle");
    let failed = source
        .run(&AtomicBool::new(false))
        .expect("record sharing failure");
    assert_eq!(failed.retryable_failure_count, 1);
    assert!(!failed.dto.warnings.is_empty());
    assert_eq!(source.artifact_ids(), previous);
    drop(locked);

    let retried = source
        .run(&AtomicBool::new(false))
        .expect("retry unlocked input");
    assert_eq!(retried.retryable_failure_count, 0);
    assert_eq!(retried.dto.scanned_count, 1);
    assert_ne!(source.artifact_ids(), previous);
    assert_eq!(source.artifact_ids().len(), 1);
}
