use super::*;
use crate::tests::registry_hive as hive_fixture;
crate::native_source_test_support!();

#[test]
fn source_generation_changing_during_a_read_discards_the_result() {
    let temp = tempfile::tempdir().unwrap();
    let (case, case_root, case_conn) =
        test_support::create_case(&temp.path().join("cases"), "registry-read-generation");
    let bitlocker =
        std::sync::Arc::new(evidence_access::bitlocker_runtime::BitLockerUnlockRegistry::default());
    let preview = evidence_access::file_service::PreviewRuntimeRegistry::default();
    let registry = RegistryBrowserRegistry::default();
    let evidence = temp.path().join("evidence");
    std::fs::create_dir(&evidence).unwrap();
    let bytes = hive_fixture::hive_bytes();
    std::fs::write(evidence.join("SOFTWARE"), &bytes).unwrap();
    let conn = &case_conn;
    {
        use persistence_source::repositories::file_repo::FileRepo;
        use persistence_sqlite::repositories::datasource_repo::{
            DataSourceRepo, DataSourceStorage,
        };
        let source = domain::DataSource {
            id: domain::DataSourceId("source".into()),
            name: "source".into(),
            source_path: evidence.clone(),
            kind: domain::DataSourceKind::LogicalDirectory,
            imported_at: chrono::Utc::now(),
            provenance: domain::DataSourceProvenance::unknown(),
        };
        let mut storage = DataSourceStorage::source_db("source", Some("windows"), None);
        storage.import_state = "ready".into();
        DataSourceRepo::new(conn)
            .insert_with_storage(&case.id, &source, &storage)
            .unwrap();
        let source_conn = source_kernel::source_db::open_source_db(&case_root, &source.id).unwrap();
        DataSourceRepo::new(&source_conn)
            .upsert_source_local_metadata(&case.id, &source)
            .unwrap();
        FileRepo::new(&source_conn)
            .insert_batch(&[domain::FileEntry {
                id: domain::FileEntryId("hive".into()),
                parent_id: None,
                data_source_id: source.id.clone(),
                path: "SOFTWARE".into(),
                name: "SOFTWARE".into(),
                entry_type: domain::EntryType::File,
                size: Some(bytes.len() as u64),
                ext: None,
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
            }])
            .unwrap();
        let store = persistence_source::detail::open_store_for_write(&source_conn)
            .unwrap()
            .unwrap();
        persistence_source::fjall::replace_source_generation(
            &store,
            "source",
            "registry-browser-session",
            &"a".repeat(64),
        )
        .unwrap();
        drop(store);
        test_support::mark_source_projection_complete(&source_conn, "source");
        let ctx = RegistryBrowserContext {
            case_conn: conn,
            case_root: &case_root,
            case_id: &case.id,
            bitlocker: &bitlocker,
            preview: &preview,
        };
        let session = crate::registry_browser_service::open_hive(
            &registry,
            &ctx,
            transport::dto::RegistryHiveOpenRequestDto {
                file_id: "ds:source:hive".into(),
                view: transport::dto::RegistryHiveViewDto::Base,
            },
        )
        .unwrap();
        let result = with_session(&registry, &ctx, &session.session_id, |_| {
            preview.invalidate_source(&case.id.0, "source").unwrap();
            Ok(123u32)
        });
        assert!(matches!(result, Err(RegistryBrowserError::ExpiredSession)));
        assert_eq!(registry.stats().unwrap().session_count, 0);
    }
}
