use std::io::{self, Cursor, Read};

use evidence_core::{FileSystemDirectoryLocator, FileSystemFileLocator, FileSystemReader, FsNode};
use persistence_sqlite::repositories::filesystem_locator_repo::FilesystemLocatorRepo;

use super::*;

const CATALOG_FINGERPRINT: &str =
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

struct LocatorFilesystem {
    directory_locators: Vec<FileSystemDirectoryLocator>,
    file_locators: Vec<FileSystemFileLocator>,
}

impl FileSystemReader for LocatorFilesystem {
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
        self.directory_locators.clone()
    }

    fn file_locators(&self) -> Vec<FileSystemFileLocator> {
        self.file_locators.clone()
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

fn image_candidate() -> ImageFilesystemCandidate {
    ImageFilesystemCandidate {
        partition_index: Some(2),
        partition_name: Some("root".to_string()),
        kind: crate::datasource_service::ImageFilesystemKind::Xfs,
        offset: 4096,
        length: None,
        source: crate::datasource_service::ImageFilesystemSource::LvmLogicalVolume,
        lvm_identity: None,
    }
}

#[test]
fn enumeration_persists_sorted_xfs_directory_and_file_locators() {
    let connection = source_connection();
    let source_id = DataSourceId("source-1".to_string());
    let candidate =
        preview_candidate_for_locator(&image_candidate()).expect("build preview candidate");
    let locator_scope = derived_filesystem_locator_scope(CATALOG_FINGERPRINT, &candidate)
        .expect("build locator scope");
    let filesystem = LocatorFilesystem {
        directory_locators: vec![
            FileSystemDirectoryLocator {
                path: "var/www".to_string(),
                locator: "256".to_string(),
            },
            FileSystemDirectoryLocator {
                path: "etc".to_string(),
                locator: "128".to_string(),
            },
        ],
        file_locators: vec![
            FileSystemFileLocator {
                path: "var/www/index.html".to_string(),
                locator: "257".to_string(),
            },
            FileSystemFileLocator {
                path: "etc/hosts".to_string(),
                locator: "129".to_string(),
            },
        ],
    };

    persist_filesystem_locators(
        &connection,
        &source_id,
        &candidate,
        &locator_scope,
        &filesystem,
    )
    .expect("persist enumeration filesystem locators");

    let stored = FilesystemLocatorRepo::new(&connection)
        .list_directory_locators(&source_id.0, 2, "XFS", &locator_scope)
        .expect("load persisted directory locators");
    assert_eq!(
        stored
            .iter()
            .map(|record| record.path.as_str())
            .collect::<Vec<_>>(),
        vec!["etc", "var/www"]
    );
    let stored_files = FilesystemLocatorRepo::new(&connection)
        .list_file_locators(&source_id.0, 2, "XFS", &locator_scope)
        .expect("load persisted file locators");
    assert_eq!(
        stored_files
            .iter()
            .map(|record| record.path.as_str())
            .collect::<Vec<_>>(),
        vec!["etc/hosts", "var/www/index.html"]
    );
}

#[test]
fn derived_locator_scope_changes_with_catalog_or_partition_identity() {
    let candidate =
        preview_candidate_for_locator(&image_candidate()).expect("build preview candidate");
    let first = derived_filesystem_locator_scope(CATALOG_FINGERPRINT, &candidate)
        .expect("build first scope");
    let changed_catalog = derived_filesystem_locator_scope(
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        &candidate,
    )
    .expect("build changed Catalog scope");
    let mut changed_partition = candidate;
    changed_partition.partition_index += 1;
    let changed_partition =
        derived_filesystem_locator_scope(CATALOG_FINGERPRINT, &changed_partition)
            .expect("build changed partition scope");

    assert_ne!(first, changed_catalog);
    assert_ne!(first, changed_partition);
}
