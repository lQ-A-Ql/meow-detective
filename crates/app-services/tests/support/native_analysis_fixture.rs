#![allow(dead_code)]

use domain::{
    DataSource, DataSourceId, DataSourceKind, DataSourceProvenance, FileEntry, FileEntryId,
};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use rusqlite::Connection;
use std::ops::Deref;

pub(crate) struct NativeAnalysisFixture {
    _root: tempfile::TempDir,
    connection: Connection,
    source_id: String,
}

impl NativeAnalysisFixture {
    pub(crate) fn new(source_id: &str, case_id: &str) -> Self {
        let root = tempfile::tempdir().expect("create native analysis root");
        let case_root = root.path().join("case");
        std::fs::create_dir_all(case_root.join("artifacts/objects"))
            .expect("create analysis artifact storage");
        let source_dir = case_root.join("sources").join(source_id);
        std::fs::create_dir_all(&source_dir).expect("create native analysis source directory");
        let database_path = source_dir.join("source.db");
        let connection = persistence_sqlite::open_or_create_source(&database_path)
            .expect("open native analysis source database");
        persistence_source::detail::configure_source_identity(&connection, source_id)
            .expect("configure native analysis source identity");
        persistence_source::detail::configure_source_fjall(&connection, &database_path)
            .expect("configure native analysis source details");
        let source = DataSource {
            id: DataSourceId(source_id.to_owned()),
            name: source_id.to_owned(),
            kind: DataSourceKind::LogicalDirectory,
            source_path: root.path().join("evidence"),
            imported_at: chrono::Utc::now(),
            provenance: DataSourceProvenance::unknown(),
        };
        DataSourceRepo::new(&connection)
            .upsert_source_local_metadata(&domain::CaseId(case_id.to_owned()), &source)
            .expect("persist native analysis source metadata");
        let fixture = Self {
            _root: root,
            connection,
            source_id: source_id.to_owned(),
        };
        fixture.refresh_generation();
        fixture
    }

    pub(crate) fn insert_file(&self, entry: FileEntry) {
        persistence_source::repositories::file_repo::FileRepo::new(&self.connection)
            .insert_batch(&[entry])
            .expect("insert native analysis catalog entry");
        self.refresh_generation();
    }

    pub(crate) fn insert_partitioned_file(&self, entry: FileEntry, partition_index: usize) {
        persistence_source::detail::upsert_files_with_partition(
            &self.connection,
            &[entry],
            partition_index,
        )
        .expect("insert partitioned native analysis catalog entry");
        self.refresh_generation();
    }

    pub(crate) fn set_encryption(&self, id: &str, encrypted: Option<bool>) {
        self.update_record(id, |record| record.encrypted = encrypted);
    }

    pub(crate) fn update_file_metadata(&self, id: &str, size: u64, hash_sha256: String) {
        self.update_record(id, |record| {
            record.size = Some(size);
            record.hash_sha256 = Some(hash_sha256);
        });
    }

    pub(crate) fn reject_partition_index_above_u32(&self, entry: FileEntry) {
        persistence_source::detail::upsert_files_with_partition(
            &self.connection,
            &[entry],
            u32::MAX as usize + 1,
        )
        .expect_err("partition index above the native range must fail");
    }

    fn update_record(
        &self,
        id: &str,
        update: impl FnOnce(&mut persistence_fjall::FileEntryRecord),
    ) {
        let store = persistence_source::detail::open_store_for_write(&self.connection)
            .expect("open native analysis store")
            .expect("configured native analysis store");
        let records = persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
        let snapshot = records.read_snapshot();
        let mut record = records
            .get(&snapshot, &self.source_id, id)
            .expect("read native analysis record")
            .expect("native analysis record");
        update(&mut record);
        records
            .upsert(&record, persistence_fjall::PersistMode::SyncAll)
            .expect("update native analysis record");
        self.refresh_generation();
    }

    fn refresh_generation(&self) {
        let store = persistence_source::detail::open_store_for_write(&self.connection)
            .expect("open native analysis store")
            .expect("configured native analysis store");
        persistence_fjall::replace_source_generation(
            &store,
            &self.source_id,
            "native-analysis-fixture",
            &"a".repeat(64),
        )
        .expect("refresh native analysis generation");
        let key = persistence_fjall::encode_key(
            "projection",
            &[persistence_fjall::KeyPart::Text(self.source_id.clone())],
        )
        .expect("encode native projection key");
        let marker = persistence_fjall::VersionedEnvelope::new(
            "source_projection_complete",
            1,
            format!("projection:{}", self.source_id),
            &self.source_id,
            1,
            self.source_id.as_bytes().to_vec(),
        );
        store
            .write_envelope(
                persistence_fjall::KeyspaceName::Meta,
                key,
                &marker,
                persistence_fjall::PersistMode::SyncAll,
            )
            .expect("mark native analysis projection complete");
    }
}

impl Deref for NativeAnalysisFixture {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        &self.connection
    }
}

pub(crate) fn source_file(
    source_id: &str,
    id: &str,
    path: &str,
    size: u64,
    encrypted: bool,
) -> FileEntry {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    FileEntry {
        id: FileEntryId(id.to_owned()),
        parent_id: None,
        data_source_id: DataSourceId(source_id.to_owned()),
        path: path.to_owned(),
        name: name.to_owned(),
        entry_type: domain::EntryType::File,
        size: Some(size),
        ext: path.rsplit_once('.').map(|(_, ext)| ext.to_owned()),
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
