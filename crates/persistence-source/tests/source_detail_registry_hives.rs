use domain::DataSourceId;
use persistence_source::repositories::file_repo::FileRepo;

fn source_connection(root: &std::path::Path) -> rusqlite::Connection {
    let connection = persistence_source::control::opening::open_source_database(
        &root.join("source.db"),
        "source-1",
    )
    .unwrap();
    persistence_source::detail::open_store_for_write(&connection)
        .unwrap()
        .expect("initialize Fjall source generation");
    connection
}

#[test]
fn registry_log_companions_use_exact_catalog_paths() {
    let root = tempfile::tempdir().unwrap();
    let main = source_connection(root.path());
    let stage =
        persistence_sqlite::connection::open_staging(&root.path().join("registry-stage.db"))
            .unwrap();
    stage
        .execute_batch(
            "INSERT INTO file_entries(id,parent_id,data_source_id,path,name,entry_type,partition_index,encrypted) VALUES
             ('source-root',NULL,'source-1','[P1]','/','directory',1,NULL),
             ('system','source-root','source-1','[P1]/sub/SYSTEM','SYSTEM','file',1,NULL),
             ('system-log1','source-root','source-1','[P1]/sub/SYSTEM.LOG1','SYSTEM.LOG1','file',1,NULL),
             ('unrelated','source-root','source-1','[P1]/sub/SYSTEM.LOG3','SYSTEM.LOG3','file',1,NULL);",
        )
        .unwrap();
    persistence_source::staging::merge_staging_catalog(&main, &stage, "source-1", 1, "Partition 1")
        .unwrap();
    let companions = FileRepo::new(&main)
        .registry_log_companions(&DataSourceId("source-1".into()), "[P1]/sub/SYSTEM")
        .unwrap();
    assert_eq!(companions.len(), 1);
    assert_eq!(companions[0].name, "SYSTEM.LOG1");
}
