#![allow(dead_code)]

use domain::{
    DataSource, DataSourceId, DataSourceKind, DataSourceProvenance, EntryType, FileEntry,
    FileEntryId,
};
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};
use std::path::Path;
pub struct EvidenceCase {
    pub temp: tempfile::TempDir,
    pub active: app_services::active_case::ActiveCase,
}
impl EvidenceCase {
    pub fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let active = app_services::case_service::create_case(
            &temp.path().join("cases"),
            "evidence-test",
            None,
        )
        .unwrap();
        Self { temp, active }
    }
    pub fn source(
        &self,
        conn: &rusqlite::Connection,
        id: &str,
        path: &Path,
        kind: DataSourceKind,
    ) -> rusqlite::Connection {
        let source = DataSource {
            id: DataSourceId(id.into()),
            name: id.into(),
            source_path: path.to_path_buf(),
            kind,
            imported_at: chrono::Utc::now(),
            provenance: DataSourceProvenance::unknown(),
        };
        let mut storage = DataSourceStorage::source_db(id, Some("windows"), None);
        storage.import_state = "ready".into();
        DataSourceRepo::new(conn)
            .insert_with_storage(&self.active.meta.id, &source, &storage)
            .unwrap();
        let local =
            app_services::source_db::open_source_db(&self.active.case_root, &source.id).unwrap();
        DataSourceRepo::new(&local)
            .upsert_source_local_metadata(&self.active.meta.id, &source)
            .unwrap();
        publish_source(&local, &source.id);
        local
    }
}
pub fn entry(conn: &rusqlite::Connection, source: &str, id: &str, path: &str, size: u64) {
    entry_with_metadata(conn, source, id, path, size, EntryType::File, None);
}

pub fn entry_with_metadata(
    conn: &rusqlite::Connection,
    source: &str,
    id: &str,
    path: &str,
    size: u64,
    entry_type: EntryType,
    unix_mode: Option<u32>,
) {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let partition_index = path
        .strip_prefix("[P")
        .and_then(|rest| rest.split_once(']'))
        .and_then(|(index, _)| index.parse::<u32>().ok());
    FileRepo::new(conn)
        .insert_batch(&[FileEntry {
            id: FileEntryId(id.to_string()),
            parent_id: None,
            data_source_id: DataSourceId(source.to_string()),
            path: path.to_string(),
            name: name.to_string(),
            entry_type,
            size: Some(size),
            ext: path.rsplit_once('.').map(|(_, ext)| ext.to_string()),
            deleted: false,
            hidden: false,
            system: false,
            encrypted: false,
            read_only: false,
            archive: false,
            unix_mode,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        }])
        .unwrap();
    if let Some(partition_index) = partition_index {
        FileRepo::new(conn)
            .set_partition_index_by_id(&FileEntryId(id.to_string()), partition_index as usize)
            .unwrap();
    }
    publish_source(conn, &DataSourceId(source.to_string()));
}

fn publish_source(conn: &rusqlite::Connection, source_id: &DataSourceId) {
    let store = persistence_source::detail::open_store_for_write(conn)
        .unwrap()
        .unwrap();
    persistence_fjall::replace_source_generation(&store, &source_id.0, "fixture", &"a".repeat(64))
        .unwrap();
    let key = persistence_fjall::encode_key(
        "projection",
        &[persistence_fjall::KeyPart::Text(source_id.0.clone())],
    )
    .unwrap();
    let marker = persistence_fjall::VersionedEnvelope::new(
        "source_projection_complete",
        1,
        format!("projection:{}", source_id.0),
        &source_id.0,
        1,
        source_id.0.as_bytes().to_vec(),
    );
    store
        .write_envelope(
            persistence_fjall::KeyspaceName::Meta,
            key,
            &marker,
            persistence_fjall::PersistMode::SyncAll,
        )
        .unwrap();
}
