use std::{
    io::{self, Cursor, Read},
    sync::{Arc, Mutex},
};

use evidence_core::{FileSystemDirectoryLocator, FileSystemFileLocator, FsNode};
use persistence_sqlite::repositories::filesystem_locator_repo::FilesystemLocatorRepo;

use super::*;
use crate::file_service::filesystem_locators::{
    restore_directory_locators, restore_file_locators, RestoredFilesystemLocatorCounts,
};

const CATALOG_FINGERPRINT: &str =
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

struct MutableLocatorFilesystem {
    directory_locators: Arc<Mutex<Vec<FileSystemDirectoryLocator>>>,
    file_locators: Arc<Mutex<Vec<FileSystemFileLocator>>>,
    reject_directory_seed: bool,
    reject_file_seed: bool,
}

impl FileSystemReader for MutableLocatorFilesystem {
    fn root(&self) -> io::Result<FsNode> {
        Ok(evidence_core::filesystem::root_node())
    }

    fn list_children(&self, _path: &str) -> io::Result<Vec<FsNode>> {
        Ok(Vec::new())
    }

    fn open_file(&self, _path: &str) -> io::Result<Box<dyn Read>> {
        Ok(Box::new(Cursor::new(Vec::new())))
    }

    fn directory_locators(&self) -> Vec<FileSystemDirectoryLocator> {
        self.directory_locators
            .lock()
            .expect("lock directory locators")
            .clone()
    }

    fn seed_directory_locators(&self, locators: &[FileSystemDirectoryLocator]) -> io::Result<()> {
        if self.reject_directory_seed {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "injected invalid directory locator",
            ));
        }
        *self
            .directory_locators
            .lock()
            .expect("lock directory locators") = locators.to_vec();
        Ok(())
    }

    fn file_locators(&self) -> Vec<FileSystemFileLocator> {
        self.file_locators
            .lock()
            .expect("lock file locators")
            .clone()
    }

    fn seed_file_locators(&self, locators: &[FileSystemFileLocator]) -> io::Result<()> {
        if self.reject_file_seed {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "injected invalid file locator",
            ));
        }
        *self.file_locators.lock().expect("lock file locators") = locators.to_vec();
        Ok(())
    }

    fn data_source_name(&self) -> &str {
        "locator-test"
    }
}

fn source_connection() -> rusqlite::Connection {
    let connection = persistence_sqlite::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&connection).expect("run source migrations");
    connection
}

fn candidate() -> PreviewPartitionCandidate {
    PreviewPartitionCandidate {
        partition_index: 2,
        filesystem_kind: "XFS".to_string(),
        offset: 4096,
        lvm_identity: None,
    }
}

fn locator_scope() -> String {
    derived_filesystem_locator_scope(CATALOG_FINGERPRINT, &candidate())
        .expect("build locator scope")
}

#[test]
fn resolved_path_cache_is_bounded_for_large_analysis_scans() {
    let mut cache = DerivedSourceReadCache::default();
    for index in 0..=MAX_RESOLVED_PATH_CACHE_ENTRIES {
        cache.cache_resolved_path(
            format!("file-{index}"),
            ResolvedFilePath {
                filesystem_key: "xfs".to_string(),
                path: format!("var/www/{index}.php"),
            },
        );
    }

    assert_eq!(cache.resolved_paths.len(), 1);
    assert!(cache
        .resolved_paths
        .contains_key(&format!("file-{MAX_RESOLVED_PATH_CACHE_ENTRIES}")));
}

#[test]
fn invalid_persisted_locator_payload_falls_back_without_seeding() {
    let connection = source_connection();
    let source_id = domain::DataSourceId("source-1".to_string());
    let locator_scope = locator_scope();
    connection
        .execute(
            "INSERT INTO source_meta (key, value) VALUES (?1, 'not-json')",
            [format!(
                "filesystem_directory_locators:v2:{}:{}:{}:{}:{}:{}",
                source_id.0.len(),
                source_id.0,
                2,
                "xfs".len(),
                "xfs",
                locator_scope
            )],
        )
        .expect("insert corrupt locator payload");
    let filesystem = MutableLocatorFilesystem {
        directory_locators: Arc::new(Mutex::new(Vec::new())),
        file_locators: Arc::new(Mutex::new(Vec::new())),
        reject_directory_seed: false,
        reject_file_seed: false,
    };

    assert_eq!(
        restore_directory_locators(
            &connection,
            &source_id,
            &candidate(),
            &locator_scope,
            &filesystem,
        ),
        0
    );
    assert!(filesystem
        .directory_locators
        .lock()
        .expect("lock directory locators")
        .is_empty());
}

#[test]
fn locator_flush_persists_only_when_discovery_count_increases() {
    let connection = source_connection();
    let source_id = domain::DataSourceId("source-1".to_string());
    let filesystem_key = "xfs-root".to_string();
    let locator_scope = locator_scope();
    let directory_locators = Arc::new(Mutex::new(vec![
        FileSystemDirectoryLocator {
            path: "etc".to_string(),
            locator: "128".to_string(),
        },
        FileSystemDirectoryLocator {
            path: "var/www".to_string(),
            locator: "256".to_string(),
        },
    ]));
    let file_locators = Arc::new(Mutex::new(vec![
        FileSystemFileLocator {
            path: "etc/hosts".to_string(),
            locator: "129".to_string(),
        },
        FileSystemFileLocator {
            path: "var/www/index.html".to_string(),
            locator: "257".to_string(),
        },
    ]));
    let mut cache = DerivedSourceReadCache::default();
    cache.filesystems.insert(
        filesystem_key.clone(),
        Box::new(MutableLocatorFilesystem {
            directory_locators,
            file_locators,
            reject_directory_seed: false,
            reject_file_seed: false,
        }),
    );
    cache
        .filesystem_candidates
        .insert(filesystem_key.clone(), candidate());
    cache
        .filesystem_locator_scopes
        .insert(filesystem_key.clone(), locator_scope.clone());
    cache
        .persisted_locator_counts
        .insert(filesystem_key, RestoredFilesystemLocatorCounts::default());

    cache
        .flush_filesystem_locators(&connection, &source_id)
        .expect("persist discovered locators");
    assert_eq!(
        FilesystemLocatorRepo::new(&connection)
            .list_directory_locators(&source_id.0, 2, "xfs", &locator_scope)
            .expect("load persisted locators")
            .len(),
        2
    );
    assert_eq!(
        FilesystemLocatorRepo::new(&connection)
            .list_file_locators(&source_id.0, 2, "xfs", &locator_scope)
            .expect("load persisted file locators")
            .len(),
        2
    );

    connection
        .execute_batch(
            "CREATE TRIGGER reject_locator_rewrite
             BEFORE UPDATE ON source_meta
             WHEN OLD.key LIKE 'filesystem_directory_locators:%'
               OR OLD.key LIKE 'filesystem_file_locators:%'
             BEGIN
                 SELECT RAISE(ABORT, 'unexpected locator rewrite');
             END;",
        )
        .expect("install locator rewrite guard");
    cache
        .flush_filesystem_locators(&connection, &source_id)
        .expect("unchanged locators should not be rewritten");
}

#[test]
fn rejected_locator_seed_is_not_marked_as_persisted() {
    let connection = source_connection();
    let source_id = domain::DataSourceId("source-1".to_string());
    let locator_scope = locator_scope();
    FilesystemLocatorRepo::new(&connection)
        .replace_directory_locators(
            &source_id.0,
            2,
            "xfs",
            &locator_scope,
            &[persistence_sqlite::repositories::filesystem_locator_repo::FilesystemDirectoryLocatorRecord {
                path: "etc".to_string(),
                locator: "128".to_string(),
            }],
        )
        .expect("persist locator");
    let filesystem = MutableLocatorFilesystem {
        directory_locators: Arc::new(Mutex::new(Vec::new())),
        file_locators: Arc::new(Mutex::new(Vec::new())),
        reject_directory_seed: true,
        reject_file_seed: false,
    };

    assert_eq!(
        restore_directory_locators(
            &connection,
            &source_id,
            &candidate(),
            &locator_scope,
            &filesystem,
        ),
        0
    );
}

#[test]
fn rejected_file_locator_seed_is_not_marked_as_persisted() {
    let connection = source_connection();
    let source_id = domain::DataSourceId("source-1".to_string());
    let locator_scope = locator_scope();
    FilesystemLocatorRepo::new(&connection)
        .replace_file_locators(
            &source_id.0,
            2,
            "xfs",
            &locator_scope,
            &[persistence_sqlite::repositories::filesystem_locator_repo::FilesystemFileLocatorRecord {
                path: "etc/hosts".to_string(),
                locator: "129".to_string(),
            }],
        )
        .expect("persist file locator");
    let filesystem = MutableLocatorFilesystem {
        directory_locators: Arc::new(Mutex::new(Vec::new())),
        file_locators: Arc::new(Mutex::new(Vec::new())),
        reject_directory_seed: false,
        reject_file_seed: true,
    };

    assert_eq!(
        restore_file_locators(
            &connection,
            &source_id,
            &candidate(),
            &locator_scope,
            &filesystem,
        ),
        0
    );
}
