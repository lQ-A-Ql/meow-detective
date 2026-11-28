use std::path::PathBuf;

use chrono::Utc;
use domain::{
    structure::{AcquisitionId, SourceCaptureRef, SourceRevisionContent},
    DataSource, DataSourceId, DataSourceKind, DataSourceProvenance, EntryType, FileEntry,
    FileEntryId,
};
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::repositories::{
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    source_revision_repo::SourceRevisionRepo,
};
use sha2::{Digest, Sha256};

use super::fixture::Fixture;

pub fn logical_source(fixture: &Fixture, id: &str, files: &[(&str, &[u8])]) -> SourceCaptureRef {
    logical_source_with_platform(fixture, id, files, None)
}

pub fn logical_source_with_platform(
    fixture: &Fixture,
    id: &str,
    files: &[(&str, &[u8])],
    platform: Option<&str>,
) -> SourceCaptureRef {
    let input = fixture.root.path().join(id);
    std::fs::create_dir_all(&input).unwrap();
    let source = DataSource {
        id: DataSourceId(id.into()),
        name: id.into(),
        kind: DataSourceKind::LogicalDirectory,
        source_path: input.clone(),
        imported_at: Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(id, platform, None);
    storage.import_state = "ready".into();
    DataSourceRepo::new(&fixture.connection)
        .insert_with_storage(&fixture.case_id, &source, &storage)
        .unwrap();
    let control =
        source_kernel::source_db::open_source_db(fixture.root.path(), &source.id).unwrap();
    DataSourceRepo::new(&control)
        .upsert_source_local_metadata(&fixture.case_id, &source)
        .unwrap();
    let mut content = Sha256::new();
    for (index, (path, bytes)) in files.iter().enumerate() {
        let host = input.join(path);
        std::fs::create_dir_all(host.parent().unwrap()).unwrap();
        std::fs::write(host, bytes).unwrap();
        content.update(path.as_bytes());
        content.update(bytes);
        FileRepo::new(&control)
            .insert_batch(&[FileEntry {
                id: FileEntryId(format!("{id}-file-{index}")),
                parent_id: None,
                data_source_id: source.id.clone(),
                path: (*path).into(),
                name: PathBuf::from(path)
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .into(),
                entry_type: EntryType::File,
                size: Some(bytes.len() as u64),
                ext: None,
                deleted: false,
                hidden: false,
                system: false,
                encrypted: false,
                read_only: true,
                archive: false,
                unix_mode: Some(0o644),
                created_at: None,
                modified_at: None,
                accessed_at: None,
                changed_at: None,
                hash_sha256: Some(hex::encode(Sha256::digest(bytes))),
            }])
            .unwrap();
    }
    let digest = hex::encode(content.finalize());
    let acquisition = AcquisitionId::new(format!("capture:{id}")).unwrap();
    let reference = SourceRevisionRepo::new(&fixture.connection)
        .bind(
            &fixture.case_id.0,
            id,
            &SourceRevisionContent::observed(
                acquisition.clone(),
                files.iter().map(|(_, bytes)| bytes.len() as u64).sum(),
                Some(digest.clone()),
            ),
            true,
        )
        .unwrap();
    let store = persistence_source::detail::open_store_for_write(&control)
        .unwrap()
        .unwrap();
    persistence_source::fjall::replace_source_generation(&store, id, "import", &digest).unwrap();
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
