use app_services::{
    datasource_service::{ImageFilesystemCandidate, ImageFilesystemKind, ImageFilesystemSource},
    import_pipeline::{enumerate_partition_with_fs, PartitionEnumerationRequest},
};
use evidence_core::{filesystem::root_node, FileSystemReader, FsNode};
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
        encrypted: false,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
    }
}

#[test]
fn candidate_without_preseeded_placeholder_materializes_partition_subtree() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_source_all(&conn).unwrap();
    let data_source_id = domain::DataSourceId("ds-candidate".to_string());
    let candidate = ImageFilesystemCandidate {
        partition_index: Some(4),
        partition_name: None,
        kind: ImageFilesystemKind::Xfs,
        offset: 0,
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
    let rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM file_entries WHERE partition_index = 4",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rows, 3);
    let missing: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM file_entries WHERE partition_index IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(missing, 0);
}
