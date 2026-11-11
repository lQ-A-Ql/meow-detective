use app_services::file_service::enumerate_filesystem_parallel_with_factory;
use domain::DataSourceId;
use evidence_core::{filesystem::fs_node_without_timestamps, FileSystemReader, FsNode, ReadSeek};
use persistence_source::repositories::file_repo::FileRepo;
use std::collections::BTreeMap;
use std::io;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

struct FixtureFs {
    tree: BTreeMap<String, Vec<(String, bool, u64)>>,
}

impl FixtureFs {
    fn new() -> Self {
        Self {
            tree: BTreeMap::from([
                (
                    String::new(),
                    vec![
                        ("etc".to_string(), true, 0),
                        ("boot.txt".to_string(), false, 3),
                    ],
                ),
                (
                    "etc".to_string(),
                    vec![
                        ("hostname".to_string(), false, 6),
                        ("network".to_string(), true, 0),
                    ],
                ),
                (
                    "etc/network".to_string(),
                    vec![("interfaces".to_string(), false, 9)],
                ),
            ]),
        }
    }
}

impl FileSystemReader for FixtureFs {
    fn root(&self) -> io::Result<FsNode> {
        Ok(fs_node_without_timestamps("fixture", true, 0))
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        Ok(self
            .tree
            .get(path)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|(name, is_dir, size)| {
                let child_path = if path.is_empty() {
                    name.clone()
                } else {
                    format!("{path}/{name}")
                };
                let mut node = fs_node_without_timestamps(name, is_dir, size);
                node.path = child_path;
                node
            })
            .collect())
    }

    fn open_file(&self, _path: &str) -> io::Result<Box<dyn io::Read>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "fixture has no file reader",
        ))
    }

    fn open_file_seekable(&self, _path: &str) -> io::Result<Box<dyn ReadSeek>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "fixture has no file reader",
        ))
    }

    fn data_source_name(&self) -> &str {
        "fixture"
    }
}

struct FaultyFs {
    inner: FixtureFs,
    unreadable_path: String,
}

impl FileSystemReader for FaultyFs {
    fn root(&self) -> io::Result<FsNode> {
        self.inner.root()
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        if path == self.unreadable_path {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "fixture denied",
            ));
        }
        self.inner.list_children(path)
    }

    fn open_file(&self, path: &str) -> io::Result<Box<dyn io::Read>> {
        self.inner.open_file(path)
    }

    fn open_file_seekable(&self, path: &str) -> io::Result<Box<dyn ReadSeek>> {
        self.inner.open_file_seekable(path)
    }

    fn data_source_name(&self) -> &str {
        self.inner.data_source_name()
    }
}

struct WideFs {
    width: usize,
}

impl FileSystemReader for WideFs {
    fn root(&self) -> io::Result<FsNode> {
        Ok(fs_node_without_timestamps("wide", true, 0))
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        if !path.is_empty() {
            return Ok(Vec::new());
        }
        Ok((0..self.width)
            .map(|index| {
                let name = format!("file-{index:04}.bin");
                let mut node = fs_node_without_timestamps(name.clone(), false, 1);
                node.path = name;
                node
            })
            .collect())
    }

    fn open_file(&self, _path: &str) -> io::Result<Box<dyn io::Read>> {
        Ok(Box::new(io::Cursor::new(Vec::new())))
    }

    fn open_file_seekable(&self, _path: &str) -> io::Result<Box<dyn ReadSeek>> {
        Ok(Box::new(io::Cursor::new(Vec::new())))
    }

    fn data_source_name(&self) -> &str {
        "wide"
    }
}

fn source_entry_count(connection: &rusqlite::Connection, source_id: &DataSourceId) -> usize {
    FileRepo::new(connection)
        .find_by_data_source(source_id)
        .expect("count file entries")
        .len()
}

fn native_source(root: &std::path::Path, source_id: &str) -> rusqlite::Connection {
    let id = DataSourceId(source_id.to_string());
    let connection = app_services::source_db::open_source_db(root, &id).unwrap();
    connection
        .execute(
            "INSERT INTO data_sources
             (id, case_id, name, kind, source_path, imported_at)
             VALUES (?1, 'case-1', ?1, 'directory', 'fixture',
                     '2026-01-01T00:00:00Z')",
            [source_id],
        )
        .unwrap();
    connection
}

#[test]
fn parallel_enumeration_preserves_directory_relationships() {
    let root = tempfile::tempdir().unwrap();
    let source_id = DataSourceId("parallel-fixture".to_string());
    let connection = native_source(root.path(), &source_id.0);
    let factory = Arc::new(|| Ok(Box::new(FixtureFs::new()) as Box<dyn FileSystemReader>));

    let stats = enumerate_filesystem_parallel_with_factory(
        &connection,
        &source_id,
        3,
        factory,
        None,
        None,
        None,
    )
    .expect("enumerate fixture filesystem");

    assert_eq!(stats.file_count, 3);
    assert_eq!(stats.dir_count, 3);
    assert_eq!(stats.total_size, 18);
    let rows = FileRepo::new(&connection)
        .find_by_data_source(&source_id)
        .unwrap()
        .into_iter()
        .map(|entry| entry.path)
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 6);
    assert!(rows.iter().any(|path| path == "etc/network/interfaces"));
    assert!(rows.iter().any(|path| path == "etc/hostname"));
}

#[test]
fn unreadable_directory_is_recorded_and_sibling_files_are_kept() {
    let root = tempfile::tempdir().unwrap();
    let source_id = DataSourceId("parallel-fault-fixture".to_string());
    let connection = native_source(root.path(), &source_id.0);
    let factory = Arc::new(|| {
        Ok(Box::new(FaultyFs {
            inner: FixtureFs::new(),
            unreadable_path: "etc/network".to_string(),
        }) as Box<dyn FileSystemReader>)
    });

    let stats = enumerate_filesystem_parallel_with_factory(
        &connection,
        &source_id,
        3,
        factory,
        None,
        None,
        None,
    )
    .expect("unreadable directory should not abort enumeration");

    assert_eq!(stats.file_count, 2);
    assert!(stats.diagnostics.iter().any(|diagnostic| diagnostic.kind
        == evidence_core::FileSystemDiagnosticKind::DirectoryUnreadable
        && diagnostic.path.as_deref() == Some("etc/network")));
    assert_eq!(source_entry_count(&connection, &source_id), 5);
}

#[test]
fn pre_cancelled_enumeration_does_not_publish_root() {
    let root = tempfile::tempdir().unwrap();
    let source_id = DataSourceId("parallel-cancelled-fixture".to_string());
    let connection = native_source(root.path(), &source_id.0);
    let cancel = AtomicBool::new(true);
    let factory = Arc::new(|| Ok(Box::new(FixtureFs::new()) as Box<dyn FileSystemReader>));

    let result = enumerate_filesystem_parallel_with_factory(
        &connection,
        &source_id,
        3,
        factory,
        None,
        None,
        Some(&cancel),
    );

    let error = result.err().expect("pre-cancelled enumeration must fail");
    assert!(error.to_string().contains("cancelled"));
    assert!(!root
        .path()
        .join("sources")
        .join(&source_id.0)
        .join("source.fjall")
        .exists());
}

#[test]
fn worker_factory_failure_returns_without_leaking_entries() {
    let root = tempfile::tempdir().unwrap();
    let source_id = DataSourceId("parallel-factory-failure".to_string());
    let connection = native_source(root.path(), &source_id.0);
    let calls = Arc::new(AtomicBool::new(false));
    let calls_for_factory = Arc::clone(&calls);
    let factory = Arc::new(move || {
        if calls_for_factory.swap(true, Ordering::AcqRel) {
            Err(persistence_sqlite::DbError::System(
                "factory failed".to_string(),
            ))
        } else {
            Ok(Box::new(FixtureFs::new()) as Box<dyn FileSystemReader>)
        }
    });

    let result = enumerate_filesystem_parallel_with_factory(
        &connection,
        &source_id,
        3,
        factory,
        None,
        None,
        None,
    );

    let error = result
        .err()
        .expect("worker factory failure must fail the transaction");
    assert!(error.to_string().contains("factory failed"));
    assert_eq!(source_entry_count(&connection, &source_id), 0);
}

#[test]
fn native_enumeration_preserves_children_across_output_batches() {
    let root = tempfile::tempdir().unwrap();
    let source_id = DataSourceId("parallel-wide-batch".to_string());
    let connection = native_source(root.path(), &source_id.0);
    let factory = Arc::new(|| Ok(Box::new(WideFs { width: 1_500 }) as Box<dyn FileSystemReader>));

    let stats = enumerate_filesystem_parallel_with_factory(
        &connection,
        &source_id,
        4,
        factory,
        None,
        None,
        None,
    )
    .expect("wide directory should enumerate completely");

    assert_eq!(stats.file_count, 1_500);
    assert_eq!(source_entry_count(&connection, &source_id), 1_501);
}
