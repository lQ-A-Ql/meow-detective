use persistence_source::repositories::staging_repo::StagingRepo;

fn source_connection(root: &std::path::Path) -> rusqlite::Connection {
    let path = root.join("source.db");
    let connection =
        persistence_source::control::opening::open_source_database(&path, "source-1").unwrap();
    persistence_source::detail::open_store_for_write(&connection)
        .unwrap()
        .expect("initialize Fjall source generation");
    connection
}

fn stage_catalog(root: &std::path::Path, partition: usize) -> rusqlite::Connection {
    let stage =
        persistence_sqlite::connection::open_staging(&root.join(format!("stage-{partition}.db")))
            .unwrap();
    for (id, parent, path, name, kind) in [
        (
            format!("raw-{partition}"),
            None,
            format!("[P{partition}]"),
            "/",
            "directory",
        ),
        (
            format!("dir-{partition}"),
            Some(format!("raw-{partition}")),
            format!("[P{partition}]/sub"),
            "sub",
            "directory",
        ),
        (
            format!("file-{partition}"),
            Some(format!("dir-{partition}")),
            format!("[P{partition}]/sub/note.txt"),
            "note.txt",
            "file",
        ),
    ] {
        stage.execute("INSERT INTO file_entries(id,parent_id,data_source_id,path,name,entry_type,partition_index,encrypted) VALUES (?1,?2,'source-1',?3,?4,?5,?6,NULL)",rusqlite::params![id,parent,path,name,kind,partition]).unwrap();
    }
    stage
}

#[test]
fn catalog_staging_merge_folds_roots_and_replays_without_duplicates() {
    let root = tempfile::tempdir().unwrap();
    let main = source_connection(root.path());
    let stores = (1..=2)
        .map(|partition| (partition, stage_catalog(root.path(), partition)))
        .collect::<Vec<_>>();
    for _ in 0..2 {
        for (partition, stage) in &stores {
            assert_eq!(
                persistence_source::staging::merge_staging_catalog(
                    &main,
                    stage,
                    "source-1",
                    *partition,
                    "Partition"
                )
                .unwrap(),
                Some(2)
            );
        }
    }
    let store = persistence_source::detail::open_store(&main)
        .unwrap()
        .unwrap();
    let catalog = persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = catalog.read_snapshot();
    let page = catalog
        .list_all(
            &snapshot,
            "source-1",
            None,
            persistence_fjall::ScanDirection::Forward,
            20,
        )
        .unwrap();
    assert_eq!(page.records.len(), 6);
    assert!(!page
        .records
        .iter()
        .any(|record| record.id.starts_with("raw-")));
    for partition in [1, 2] {
        let file = catalog
            .get(&snapshot, "source-1", &format!("file-{partition}"))
            .unwrap()
            .unwrap();
        assert_eq!(file.partition_index, Some(partition));
        assert_eq!(file.path, format!("[P{partition}]/sub/note.txt"));
        assert_eq!(file.encrypted, None);
        let directory = catalog
            .get(&snapshot, "source-1", &format!("dir-{partition}"))
            .unwrap()
            .unwrap();
        assert_eq!(
            directory.parent_id,
            Some(format!("partition-root:source-1:{partition}"))
        );
    }
    let error = persistence_source::staging::merge_staging_catalog(
        &main,
        &stores[0].1,
        "source-2",
        1,
        "Partition",
    )
    .unwrap_err();
    assert!(error
        .to_string()
        .contains("catalog staging source identity mismatch"));
}

#[test]
fn analysis_staging_merge_pages_to_fjall_and_replays_without_duplicates() {
    let root = tempfile::tempdir().unwrap();
    let main = source_connection(root.path());
    let stage = StagingRepo::open_analysis_staging_conn(root.path(), "source-1", 0).unwrap();
    stage.execute("INSERT INTO artifact_rows(id,file_id,artifact_type,extractor_id,display_name,summary,data_json,created_at) VALUES ('artifact-0','file-0','Prefetch','prefetch','Program executed','observed','{\"runCount\":2}','2026-11-07T00:00:00Z')",[]).unwrap();
    stage.execute("INSERT INTO timeline_rows(id,file_id,event_type,timestamp,title,description,parser_id) VALUES ('event-0','file-0','FILE_EXECUTED','2026-11-07T00:00:00Z','Program executed','observed','prefetch')",[]).unwrap();
    for _ in 0..2 {
        assert_eq!(
            persistence_source::staging::analysis::merge(&main, &stage, "case-1", "source-1")
                .unwrap(),
            Some((1, 1))
        );
    }
    assert_eq!(
        persistence_source::repositories::artifact_repo::ArtifactRepo::new(&main)
            .count()
            .unwrap(),
        1
    );
    assert_eq!(
        persistence_source::repositories::timeline_repo::TimelineRepo::new(&main)
            .count()
            .unwrap(),
        1
    );
    let error = persistence_source::staging::analysis::merge(&main, &stage, "case-1", "source-2")
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("analysis staging source identity mismatch"));
}
