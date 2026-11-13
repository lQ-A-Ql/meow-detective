use app_services::{active_case::ActiveCase, case_service, source_db};
use chrono::Utc;
use domain::{
    DataSource, DataSourceId, DataSourceKind, DataSourceProvenance, EntryType, FileEntry,
    FileEntryId,
};
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};
use search::{SearchFileDocument, SearchIndex};
use std::path::PathBuf;

pub struct SearchCase {
    pub active: ActiveCase,
    _temporary: tempfile::TempDir,
}

impl SearchCase {
    pub fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let active =
            case_service::create_case(&temporary.path().join("case"), "search pool", None).unwrap();
        Self {
            active,
            _temporary: temporary,
        }
    }

    pub fn add_source(&self, id: &str, documents: &[SearchFileDocument]) {
        self.active
            .with_conn(|connection| {
                let source = DataSource {
                    id: DataSourceId(id.to_owned()),
                    name: id.to_owned(),
                    kind: DataSourceKind::LogicalDirectory,
                    source_path: self.active.case_root.join("evidence").join(id),
                    imported_at: Utc::now(),
                    provenance: DataSourceProvenance::unknown(),
                };
                let mut storage = DataSourceStorage::source_db(id, Some("linux"), None);
                storage.import_state = "ready".to_owned();
                DataSourceRepo::new(connection).insert_with_storage(
                    &self.active.meta.id,
                    &source,
                    &storage,
                )?;
                let local = source_db::open_source_db(&self.active.case_root, &source.id)?;
                DataSourceRepo::new(&local)
                    .upsert_source_local_metadata(&self.active.meta.id, &source)?;
                Ok(())
            })
            .unwrap();
        self.add_documents(id, documents);
    }

    pub fn add_documents(&self, id: &str, documents: &[SearchFileDocument]) {
        let source_id = DataSourceId(id.to_owned());
        let local = source_db::open_source_db(&self.active.case_root, &source_id).unwrap();
        let entries = documents
            .iter()
            .map(|document| FileEntry {
                id: FileEntryId(document.file_id.clone()),
                parent_id: None,
                data_source_id: source_id.clone(),
                path: document.path.clone(),
                name: document.name.clone(),
                entry_type: EntryType::File,
                size: document.size,
                ext: Some(document.extension.clone()),
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
            })
            .collect::<Vec<_>>();
        FileRepo::new(&local).insert_batch(&entries).unwrap();
        let store = persistence_source::detail::open_store_for_write(&local)
            .unwrap()
            .unwrap();
        persistence_source::fjall::replace_source_generation(
            &store,
            id,
            "search-test",
            &"a".repeat(64),
        )
        .unwrap();
        drop(store);
        let handle =
            source_db::open_registered_fjall_source_read_write(&self.active.case_root, &source_id)
                .unwrap();
        super::fjall_projection::mark_source_projection_complete(&handle, id);
        drop(handle);
        let index = SearchIndex::create(&self.index_path(id)).unwrap();
        let mut writer = index.metadata_writer().unwrap();
        writer.add_documents(documents).unwrap();
        writer.commit().unwrap();
    }

    pub fn index_path(&self, source_id: &str) -> PathBuf {
        self.active
            .case_root
            .join("sources")
            .join(source_id)
            .join("index")
    }
}

impl Drop for SearchCase {
    fn drop(&mut self) {
        app_services::search_service::clear_case_file_search_cache(&self.active.meta.id.0).unwrap();
    }
}

pub fn document(index: usize) -> SearchFileDocument {
    SearchFileDocument {
        file_id: format!("f{index}"),
        name: format!("{index:03}.txt"),
        path: format!("/{index:03}.txt"),
        extension: "txt".to_owned(),
        entry_type: "file".to_owned(),
        size: Some(index as u64),
        modified_at: None,
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
    }
}
