use super::*;
crate::native_source_test_support!();
use evidence_core::{filesystem::root_node, FsNode};
use std::io::{self, Cursor, Read};
use test_support::NativeSourceFixture;

struct LargeRootFs {
    child_count: usize,
}

struct DeepDirectoryFs {
    depth: usize,
}

impl FileSystemReader for LargeRootFs {
    fn root(&self) -> io::Result<FsNode> {
        Ok(root_node())
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        if !path.is_empty() {
            return Ok(Vec::new());
        }
        Ok((0..self.child_count)
            .map(|index| FsNode {
                name: format!("file-{index:04}.txt"),
                path: format!("file-{index:04}.txt"),
                is_dir: false,
                size: 1,
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
            })
            .collect())
    }

    fn open_file(&self, _path: &str) -> io::Result<Box<dyn Read>> {
        Ok(Box::new(Cursor::new(Vec::<u8>::new())))
    }

    fn data_source_name(&self) -> &str {
        "large-root"
    }
}

impl FileSystemReader for DeepDirectoryFs {
    fn root(&self) -> io::Result<FsNode> {
        Ok(root_node())
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        let current_depth = if path.is_empty() {
            0
        } else {
            path.strip_prefix("dir-")
                .and_then(|value| value.parse::<usize>().ok())
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid path"))?
        };
        if current_depth >= self.depth {
            return Ok(Vec::new());
        }
        let child_depth = current_depth + 1;
        Ok(vec![FsNode {
            name: format!("dir-{child_depth}"),
            path: format!("dir-{child_depth}"),
            is_dir: true,
            size: 0,
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
        }])
    }

    fn open_file(&self, _path: &str) -> io::Result<Box<dyn Read>> {
        Ok(Box::new(Cursor::new(Vec::<u8>::new())))
    }

    fn data_source_name(&self) -> &str {
        "deep-directory"
    }
}

fn setup_placeholder(
    connection: &rusqlite::Connection,
    data_source_id: &DataSourceId,
) -> FileEntryId {
    crate::file_service::insert_partition_placeholder_root(
        connection,
        data_source_id,
        3,
        "Partition 3 (XFS)",
        "queued",
    )
    .expect("insert placeholder")
}

#[test]
fn cancellation_keeps_only_committed_catalog_batches() {
    let data_source_id = DataSourceId("source-checkpoint".to_string());
    let fixture = NativeSourceFixture::new(&data_source_id.0, "case-checkpoint");
    let placeholder_id = setup_placeholder(&fixture.connection, &data_source_id);
    let cancel_token = AtomicBool::new(false);

    let result = replace_placeholder_root_checkpointed(
        &fixture.connection,
        &placeholder_id,
        &LargeRootFs {
            child_count: CATALOG_COMMIT_BATCH_ROWS + 1,
        },
        Some("Partition 3 (XFS)"),
        Some(&|_| cancel_token.store(true, Ordering::Relaxed)),
        &cancel_token,
    );
    let error = match result {
        Ok(_) => panic!("second batch should observe cancellation"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("cancelled"));
    assert_eq!(
        catalog_row_count(&fixture.connection, &data_source_id),
        CATALOG_COMMIT_BATCH_ROWS as u64 + 1
    );
    assert_eq!(
        unindexed_catalog_rows(&fixture.connection, &data_source_id),
        0
    );
}

#[test]
fn batches_catalog_rows_across_directory_boundaries() {
    let data_source_id = DataSourceId("source-cross-directory-batch".to_string());
    let fixture = NativeSourceFixture::new(&data_source_id.0, "case-cross-directory");
    let placeholder_id = setup_placeholder(&fixture.connection, &data_source_id);
    let cancel_token = AtomicBool::new(false);

    let result = replace_placeholder_root_checkpointed(
        &fixture.connection,
        &placeholder_id,
        &DeepDirectoryFs {
            depth: CATALOG_COMMIT_BATCH_ROWS + 1,
        },
        Some("Partition 3 (XFS)"),
        Some(&|_| cancel_token.store(true, Ordering::Relaxed)),
        &cancel_token,
    );

    let error = match result {
        Ok(_) => panic!("the second batch should observe cancellation"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("cancelled"));
    assert_eq!(
        catalog_row_count(&fixture.connection, &data_source_id),
        CATALOG_COMMIT_BATCH_ROWS as u64 + 1
    );
}

#[test]
fn completed_checkpointed_enumeration_returns_full_stats() {
    let data_source_id = DataSourceId("source-complete".to_string());
    let fixture = NativeSourceFixture::new(&data_source_id.0, "case-complete");
    let placeholder_id = setup_placeholder(&fixture.connection, &data_source_id);

    let stats = replace_placeholder_root_checkpointed(
        &fixture.connection,
        &placeholder_id,
        &LargeRootFs { child_count: 2 },
        Some("Partition 3 (XFS)"),
        None,
        &AtomicBool::new(false),
    )
    .expect("enumerate filesystem");

    assert_eq!(stats.file_count, 2);
    assert_eq!(stats.dir_count, 1);
    assert_eq!(stats.total_size, 2);
    assert!(stats.diagnostics.is_empty());
}

fn catalog_row_count(connection: &rusqlite::Connection, source_id: &DataSourceId) -> u64 {
    persistence_fjall::CatalogRecordStore::from_source_store(
        (*persistence_sqlite::source_fjall::require_source_store(connection).unwrap()).clone(),
    )
    .count(
        &persistence_sqlite::source_fjall::require_source_store(connection)
            .unwrap()
            .database()
            .read_snapshot(),
        Some(&source_id.0),
    )
    .unwrap()
}

fn unindexed_catalog_rows(connection: &rusqlite::Connection, source_id: &DataSourceId) -> u64 {
    let store = persistence_sqlite::source_fjall::require_source_store(connection).unwrap();
    let repository = persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = repository.read_snapshot();
    let mut cursor = None;
    let mut unindexed = 0;
    loop {
        let page = repository
            .list_all(
                &snapshot,
                &source_id.0,
                cursor.as_deref(),
                persistence_fjall::ScanDirection::Forward,
                512,
            )
            .unwrap();
        unindexed += page
            .records
            .iter()
            .filter(|record| record.partition_index.is_none())
            .count();
        cursor = page.next_cursor;
        if cursor.is_none() {
            return unindexed as u64;
        }
    }
}
