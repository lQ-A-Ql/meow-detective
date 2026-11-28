use std::path::{Path, PathBuf};

use chrono::Utc;
use domain::{
    structure::{
        AcquisitionId, ByteRange, ByteRangeSet, ReadDemand, ReadIntent, ReadPriority,
        ReadVerification, SourceRevisionContent, SourceRevisionRef, StructureAttemptRef,
    },
    CaseId, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance,
};
use persistence_sqlite::repositories::{
    case_repo::CaseRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    source_revision_repo::SourceRevisionRepo,
};
use sha2::{Digest, Sha256};

pub struct Fixture {
    pub root: tempfile::TempDir,
    pub conn: rusqlite::Connection,
    pub case_id: CaseId,
}

impl Fixture {
    pub fn new() -> Self {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-registered-verification");
        std::fs::create_dir_all(&workspace).unwrap();
        let root = tempfile::tempdir_in(workspace).unwrap();
        std::fs::create_dir_all(root.path().join("artifacts/objects")).unwrap();
        let conn = persistence_sqlite::open_or_create(&root.path().join("app.db")).unwrap();
        persistence_sqlite::runner::run_all(&conn).unwrap();
        let case_id = CaseId(uuid::Uuid::new_v4().to_string());
        CaseRepo::new(&conn)
            .create(&domain::CaseMeta {
                id: case_id.clone(),
                name: "Range verification".into(),
                number: None,
                examiner: None,
                notes: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .unwrap();
        Self {
            root,
            conn,
            case_id,
        }
    }

    pub fn source(
        &self,
        id: &str,
        bytes: &[u8],
        parents: &[SourceRevisionRef],
    ) -> SourceRevisionRef {
        let path = self.path(id);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        self.register(id, DataSourceKind::Raw, path, bytes, parents)
    }

    pub fn e01_source(&self, id: &str) -> (SourceRevisionRef, Vec<u8>, PathBuf) {
        use std::io::Read;
        let input = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/fixtures/public-small/e01/tiny.E01");
        let path = self.root.path().join(format!("{id}.E01"));
        std::fs::copy(input, &path).unwrap();
        let mut bytes = Vec::new();
        image_e01::E01Reader::open(&path)
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        (
            self.register(id, DataSourceKind::E01, path.clone(), &bytes, &[]),
            bytes,
            path,
        )
    }

    fn register(
        &self,
        id: &str,
        kind: DataSourceKind,
        path: PathBuf,
        bytes: &[u8],
        parents: &[SourceRevisionRef],
    ) -> SourceRevisionRef {
        let source = DataSource {
            id: DataSourceId(id.into()),
            name: id.into(),
            kind,
            source_path: path,
            imported_at: Utc::now(),
            provenance: DataSourceProvenance::unknown(),
        };
        let mut storage = DataSourceStorage::source_db(id, Some("linux"), None);
        storage.import_state = "ready".into();
        DataSourceRepo::new(&self.conn)
            .insert_with_storage(&self.case_id, &source, &storage)
            .unwrap();
        let control =
            source_kernel::source_db::open_source_db(self.root.path(), &source.id).unwrap();
        DataSourceRepo::new(&control)
            .upsert_source_local_metadata(&self.case_id, &source)
            .unwrap();
        let revisions = SourceRevisionRepo::new(&self.conn);
        if !parents.is_empty() {
            revisions.bind_inputs(&self.case_id.0, id, parents).unwrap();
        }
        let reference = revisions
            .bind(
                &self.case_id.0,
                id,
                &SourceRevisionContent::captured(
                    AcquisitionId(format!("capture:{id}")),
                    bytes.len() as u64,
                    None,
                    hex::encode(Sha256::digest(bytes)),
                ),
                true,
            )
            .unwrap();
        seal(&control, id, "initial", &hex::encode(Sha256::digest(bytes)));
        reference
    }

    pub fn path(&self, source: &str) -> PathBuf {
        self.root
            .path()
            .join("evidence")
            .join(format!("{source}.raw"))
    }

    pub fn revise(&self, source: &SourceRevisionRef) {
        let repo = SourceRevisionRepo::new(&self.conn);
        let (_, mut content) = repo.current(&self.case_id.0, &source.source_id).unwrap();
        content.metadata_identity = Some(uuid::Uuid::new_v4().to_string());
        content.metadata_identity_status = domain::structure::DigestVerification::Recorded;
        repo.bind(&self.case_id.0, &source.source_id, &content, true)
            .unwrap();
    }
}

pub fn demand(
    source: &SourceRevisionRef,
    size: u64,
    ranges: &[(u64, u64)],
    consumer: &str,
) -> ReadDemand {
    ReadDemand::new(
        source.clone(),
        format!("source-revision:{}:{}", source.source_id, source.revision),
        ReadIntent::ContentRead,
        ByteRangeSet::new(
            size,
            ranges
                .iter()
                .map(|(start, length)| ByteRange::new(*start, *length).unwrap()),
        )
        .unwrap(),
        ReadPriority::Interactive,
        None,
        true,
        vec![consumer.into()],
        ReadVerification::ContentDigest,
    )
    .unwrap()
    .with_attempt(attempt())
    .unwrap()
}

pub fn attempt() -> StructureAttemptRef {
    StructureAttemptRef {
        attempt_id: "verification-attempt".into(),
        owner: "verification-owner".into(),
    }
}

pub fn bytes() -> Vec<u8> {
    (0..16_384).map(|index| (index % 251) as u8).collect()
}

pub fn seal(control: &rusqlite::Connection, source: &str, attempt: &str, digest: &str) {
    let store = persistence_source::detail::open_store_for_write(control)
        .unwrap()
        .unwrap();
    persistence_source::fjall::replace_source_generation(&store, source, attempt, digest).unwrap();
    let key = persistence_source::fjall::encode_key(
        "projection",
        &[persistence_source::fjall::KeyPart::Text(source.into())],
    )
    .unwrap();
    store
        .write_envelope(
            persistence_source::fjall::KeyspaceName::Meta,
            key,
            &persistence_source::fjall::VersionedEnvelope::new(
                "source_projection_complete",
                1,
                format!("projection:{source}"),
                source,
                1,
                source.as_bytes().to_vec(),
            ),
            persistence_source::fjall::PersistMode::SyncAll,
        )
        .unwrap();
}

pub fn raw(path: &Path) -> Box<dyn evidence_core::EvidenceReader> {
    Box::new(evidence_core::RawImageReader::open(path).unwrap())
}
