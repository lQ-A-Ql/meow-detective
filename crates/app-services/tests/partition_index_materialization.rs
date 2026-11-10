use app_services::{
    datasource_service::{ImageFilesystemCandidate, ImageFilesystemKind, ImageFilesystemSource},
    import_pipeline::{enumerate_partition_with_fs, PartitionEnumerationRequest},
};
use evidence_core::{filesystem::root_node, FileSystemReader, FsNode};
use persistence_sqlite::repositories::file_repo::FileRepo;
use std::collections::HashMap;
use std::io::{self, Cursor, Read};

struct NestedFilesystem;

impl FileSystemReader for NestedFilesystem {
    fn root(&self) -> io::Result<FsNode> {
        Ok(root_node())
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        let children = match path {
            "" => vec![directory("etc", "etc")],
            "etc" => vec![file("hosts", "etc/hosts")],
            _ => Vec::new(),
        };
        Ok(children)
    }

    fn open_file(&self, _path: &str) -> io::Result<Box<dyn Read>> {
        Ok(Box::new(Cursor::new(Vec::<u8>::new())))
    }

    fn data_source_name(&self) -> &str {
        "nested-filesystem"
    }
}

fn directory(name: &str, path: &str) -> FsNode {
    node(name, path, true)
}

fn file(name: &str, path: &str) -> FsNode {
    node(name, path, false)
}

fn node(name: &str, path: &str, is_dir: bool) -> FsNode {
    FsNode {
        name: name.to_string(),
        path: path.to_string(),
        is_dir,
        size: if is_dir { 0 } else { 5 },
        hidden: false,
        system: false,
        read_only: false,
        encrypted: false,
        archive: false,
        unix_mode: None,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
    }
}

#[test]
fn candidate_without_preseeded_placeholder_materializes_partition_subtree() {
    let root = tempfile::tempdir().unwrap();
    let data_source_id = domain::DataSourceId("ds-candidate".to_string());
    let conn = app_services::source_db::open_source_db(root.path(), &data_source_id).unwrap();
    conn.execute(
        "INSERT INTO data_sources
         (id, case_id, name, kind, source_path, imported_at)
         VALUES (?1, 'case-1', ?1, 'e01', 'source.E01',
                 '2026-01-01T00:00:00Z')",
        [&data_source_id.0],
    )
    .unwrap();
    let store = persistence_sqlite::source_fjall::open_store_for_write(&conn)
        .unwrap()
        .unwrap();
    persistence_fjall::seal_source_generation(
        &store,
        &data_source_id.0,
        "fixture",
        &"a".repeat(64),
    )
    .unwrap();
    drop(store);
    let candidate = ImageFilesystemCandidate {
        partition_index: Some(4),
        partition_name: None,
        kind: ImageFilesystemKind::Xfs,
        offset: 0,
        length: None,
        source: ImageFilesystemSource::DirectVolume,
        lvm_identity: None,
    };

    let placeholders = HashMap::new();
    let stats = enumerate_partition_with_fs(PartitionEnumerationRequest {
        conn: &conn,
        data_source_id: &data_source_id,
        fs: &NestedFilesystem,
        root_name: "Partition 4 (XFS)",
        placeholder_roots: &placeholders,
        candidate: &candidate,
        progress_cb: None,
        cancel_token: None,
    })
    .unwrap();

    assert_eq!(stats.dir_count, 2);
    assert_eq!(stats.file_count, 1);
    let records = FileRepo::new(&conn)
        .find_by_data_source(&data_source_id)
        .unwrap();
    let rows = records
        .iter()
        .filter(|record| {
            persistence_sqlite::source_fjall::partition_index(&conn, &record.id.0).unwrap()
                == Some(4)
        })
        .count();
    assert_eq!(rows, 3);
    let missing = records
        .iter()
        .filter(|record| {
            persistence_sqlite::source_fjall::partition_index(&conn, &record.id.0)
                .unwrap()
                .is_none()
        })
        .count();
    assert_eq!(missing, 0);
}
