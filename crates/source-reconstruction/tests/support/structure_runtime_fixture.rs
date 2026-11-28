use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{atomic::AtomicBool, Mutex, MutexGuard},
};

use chrono::Utc;
use domain::{
    structure::{
        AcquisitionId, ReadPriority, SourceCaptureRef, SourceRevisionContent, StructureGoal,
        StructureRequest, StructureResourcePolicy, VersionSelection,
    },
    CaseId, CaseMeta, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance,
};
use persistence_sqlite::repositories::{
    case_repo::CaseRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    source_revision_repo::SourceRevisionRepo,
};

pub struct Fixture {
    pub root: tempfile::TempDir,
    pub connection: rusqlite::Connection,
    pub case_id: CaseId,
    _capture_gate: MutexGuard<'static, ()>,
}

static CAPTURE_GATE: Mutex<()> = Mutex::new(());

impl Fixture {
    pub fn new() -> Self {
        let capture_gate = CAPTURE_GATE.lock().unwrap();
        let parent =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-structure-runtime");
        std::fs::create_dir_all(&parent).unwrap();
        let root = tempfile::tempdir_in(parent).unwrap();
        std::fs::create_dir_all(root.path().join("artifacts/objects")).unwrap();
        let connection = persistence_sqlite::open_or_create(&root.path().join("app.db")).unwrap();
        persistence_sqlite::runner::run_all(&connection).unwrap();
        let case_id = CaseId(uuid::Uuid::new_v4().to_string());
        CaseRepo::new(&connection)
            .create(&CaseMeta {
                id: case_id.clone(),
                name: "Structure runtime".into(),
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
            case_id,
            _capture_gate: capture_gate,
        }
    }

    pub fn source(&self, id: &str, bytes: &[u8]) -> SourceCaptureRef {
        let path = self.root.path().join(format!("{id}.raw"));
        std::fs::write(&path, bytes).unwrap();
        let source = DataSource {
            id: DataSourceId(id.into()),
            name: id.into(),
            kind: DataSourceKind::Raw,
            source_path: path,
            imported_at: Utc::now(),
            provenance: DataSourceProvenance::unknown(),
        };
        let mut storage = DataSourceStorage::source_db(id, None, None);
        storage.import_state = "ready".into();
        DataSourceRepo::new(&self.connection)
            .insert_with_storage(&self.case_id, &source, &storage)
            .unwrap();
        let control =
            source_kernel::source_db::open_source_db(self.root.path(), &source.id).unwrap();
        DataSourceRepo::new(&control)
            .upsert_source_local_metadata(&self.case_id, &source)
            .unwrap();
        let content = evidence_access::source_runtime::content::capture(
            &source.source_path,
            &source.kind,
            &AtomicBool::new(false),
        )
        .unwrap();
        let acquisition = AcquisitionId::new(format!("capture:{id}")).unwrap();
        let reference = SourceRevisionRepo::new(&self.connection)
            .bind(
                &self.case_id.0,
                id,
                &SourceRevisionContent::captured(
                    acquisition.clone(),
                    bytes.len() as u64,
                    None,
                    content.logical_sha256.clone(),
                ),
                true,
            )
            .unwrap();
        evidence_access::source_runtime::content::persist_registered_content(
            &control, &reference, &content,
        )
        .unwrap();
        let store = persistence_source::detail::open_store_for_write(&control)
            .unwrap()
            .unwrap();
        persistence_source::fjall::replace_source_generation(
            &store,
            id,
            "import",
            &content.logical_sha256,
        )
        .unwrap();
        let key = persistence_source::fjall::encode_key(
            "projection",
            &[persistence_source::fjall::KeyPart::Text(id.into())],
        )
        .unwrap();
        store
            .write_envelope(
                persistence_source::fjall::KeyspaceName::Meta,
                key,
                &persistence_source::fjall::VersionedEnvelope::new(
                    "source_projection_complete",
                    1,
                    format!("projection:{id}"),
                    id,
                    1,
                    id.as_bytes().to_vec(),
                ),
                persistence_source::fjall::PersistMode::SyncAll,
            )
            .unwrap();
        SourceCaptureRef::new(reference, acquisition).unwrap()
    }

    pub fn request(&self, captures: Vec<SourceCaptureRef>) -> StructureRequest {
        StructureRequest {
            case_id: self.case_id.0.clone(),
            source_scope: captures,
            targets: vec![],
            version_selection: VersionSelection::Current,
            goals: BTreeSet::from([StructureGoal::Inventory]),
            analysis_profiles: BTreeSet::new(),
            resource_policy: StructureResourcePolicy {
                bytes: 32 * 1024 * 1024,
                readers: 2,
                handles: 4,
                cpu: 2,
                io_domains: 2,
                priority: ReadPriority::Normal,
                deadline_unix_ms: None,
                active_work_items: 64,
                page_rows: 512,
                page_bytes: 1024 * 1024,
            },
            adapter_hints: BTreeSet::new(),
        }
    }
}
