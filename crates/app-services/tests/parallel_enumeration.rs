use app_services::file_service::enumerate_filesystem_parallel_with_factory;
use domain::DataSourceId;
use evidence_core::{filesystem::fs_node_without_timestamps, FileSystemReader, FsNode, ReadSeek};
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

fn source_entry_count(connection: &rusqlite::Connection, source_id: &DataSourceId) -> usize {
    connection
        .query_row(
            "SELECT COUNT(*) FROM file_entries WHERE data_source_id = ?1",
            [&source_id.0],
            |row| row.get::<_, i64>(0),
        )
        .expect("count file entries") as usize
}

#[test]
fn parallel_enumeration_preserves_directory_relationships() {
    let connection = persistence_sqlite::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&connection).expect("run source migrations");
    let source_id = DataSourceId("parallel-fixture".to_string());
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
    let mut statement = connection
        .prepare("SELECT path FROM file_entries WHERE data_source_id = ?1")
        .expect("prepare file entry query");
    let rows = statement
        .query_map([&source_id.0], |row| row.get::<_, String>(0))
        .expect("query file entries")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect file entries");
    assert_eq!(rows.len(), 6);
    assert!(rows.iter().any(|path| path == "etc/network/interfaces"));
    assert!(rows.iter().any(|path| path == "etc/hostname"));
}

#[test]
fn unreadable_directory_is_recorded_and_sibling_files_are_kept() {
    let connection = persistence_sqlite::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&connection).expect("run source migrations");
    let source_id = DataSourceId("parallel-fault-fixture".to_string());
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
    let connection = persistence_sqlite::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&connection).expect("run source migrations");
    let source_id = DataSourceId("parallel-cancelled-fixture".to_string());
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
    assert_eq!(source_entry_count(&connection, &source_id), 0);
}

#[test]
fn worker_factory_failure_returns_without_leaking_entries() {
    let connection = persistence_sqlite::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&connection).expect("run source migrations");
    let source_id = DataSourceId("parallel-factory-failure".to_string());
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
