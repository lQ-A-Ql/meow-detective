use app_services::{case_service, file_service};
use evidence_core::LogicalFsReader;
use persistence_sqlite::repositories::{
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    file_repo::FileRepo,
};
use tempfile::TempDir;

#[test]
fn extract_file_uses_entry_reader_and_writes_destination() {
    let temporary = TempDir::new().unwrap();
    let evidence_dir = temporary.path().join("evidence");
    std::fs::create_dir_all(&evidence_dir).unwrap();
    std::fs::write(evidence_dir.join("note.txt"), b"extract me").unwrap();

    let active =
        case_service::create_case(&temporary.path().join("cases"), "extract", Some("tester"))
            .unwrap();
    let case_id = active.meta.id.clone();

    active
        .with_conn(|connection| {
            let data_source_id = domain::DataSourceId("ds-extract".to_string());
            let data_source = domain::DataSource {
                id: data_source_id.clone(),
                name: "evidence".to_string(),
                kind: domain::DataSourceKind::LogicalDirectory,
                source_path: evidence_dir.clone(),
                imported_at: chrono::Utc::now(),
                provenance: domain::DataSourceProvenance::unknown(),
            };
            let mut storage =
                DataSourceStorage::source_db(&data_source_id.0, Some("windows"), None);
            storage.import_state = "ready".to_string();
            DataSourceRepo::new(connection).insert_with_storage(
                &case_id,
                &data_source,
                &storage,
            )?;
            let source_connection =
                app_services::source_db::open_source_db(&active.case_root, &data_source_id)?;
            DataSourceRepo::new(&source_connection)
                .upsert_source_local_metadata(&case_id, &data_source)?;

            let filesystem = LogicalFsReader::open(&evidence_dir, "evidence")
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            file_service::enumerate_filesystem(&source_connection, &data_source_id, &filesystem)?;

            let local_file_id = FileRepo::new(&source_connection)
                .find_by_data_source(&data_source_id)?
                .into_iter()
                .find(|entry| entry.name == "note.txt")
                .map(|entry| entry.id.0)
                .expect("note.txt should be enumerated");
            let file_id = app_services::source_db::GlobalFileId::new(
                data_source_id,
                domain::FileEntryId(local_file_id),
            )
            .encode()
            .0;
            let destination = temporary.path().join("exports").join("note-copy.txt");
            let written = file_service::extract_file_to_destination_for_case(
                connection,
                &active.case_root,
                &active.meta.id,
                &file_id,
                &destination,
                false,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;

            assert_eq!(written, 10);
            assert_eq!(std::fs::read(&destination).unwrap(), b"extract me");

            Ok(())
        })
        .unwrap();
}
