use super::*;
#[path = "../../support/registry_browser_hive.rs"]
mod hive_fixture;

#[test]
fn source_generation_changing_during_a_read_discards_the_result() {
    let temp = tempfile::tempdir().unwrap();
    let active = crate::case_service::create_case(
        &temp.path().join("cases"),
        "registry-read-generation",
        None,
    )
    .unwrap();
    let bitlocker =
        std::sync::Arc::new(crate::bitlocker_runtime::BitLockerUnlockRegistry::default());
    let preview = crate::file_service::PreviewRuntimeRegistry::default();
    let registry = RegistryBrowserRegistry::default();
    let evidence = temp.path().join("evidence");
    std::fs::create_dir(&evidence).unwrap();
    let bytes = hive_fixture::hive_bytes();
    std::fs::write(evidence.join("SOFTWARE"), &bytes).unwrap();
    active
        .with_conn(|conn| {
            use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo,DataSourceStorage};
            let source=domain::DataSource {id:domain::DataSourceId("source".into()),name:"source".into(),source_path:evidence.clone(),kind:domain::DataSourceKind::LogicalDirectory,imported_at:chrono::Utc::now(),provenance:domain::DataSourceProvenance::unknown()};
            let mut storage=DataSourceStorage::source_db("source",Some("windows"),None);storage.import_state="ready".into();
            DataSourceRepo::new(conn).insert_with_storage(&active.meta.id,&source,&storage).unwrap();
            let source_conn=crate::source_db::open_source_db(&active.case_root,&source.id).unwrap();
            DataSourceRepo::new(&source_conn).upsert_source_local_metadata(&active.meta.id,&source).unwrap();
            source_conn.execute("INSERT INTO file_entries (id,data_source_id,path,name,entry_type,size,deleted,hidden,system,encrypted) VALUES ('hive','source','SOFTWARE','SOFTWARE','file',?1,0,0,0,0)",[bytes.len() as u64]).unwrap();
            let ctx = RegistryBrowserContext {case_conn:conn,case_root:&active.case_root,case_id:&active.meta.id,bitlocker:&bitlocker,preview:&preview};
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
                preview
                    .invalidate_source(&active.meta.id.0, "source")
                    .unwrap();
                Ok(123u32)
            });
            assert!(matches!(result, Err(RegistryBrowserError::ExpiredSession)));
            assert_eq!(registry.stats().unwrap().session_count, 0);
            Ok(())
        })
        .unwrap();
}
