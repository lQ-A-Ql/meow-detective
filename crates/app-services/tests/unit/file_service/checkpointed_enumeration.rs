use super::*;
use evidence_core::{filesystem::root_node, FsNode};
use std::io::{self, Cursor, Read};

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
    let connection = persistence_sqlite::open_in_memory().expect("open database");
    persistence_sqlite::runner::run_source_all(&connection).expect("run source migrations");
    let data_source_id = DataSourceId("source-checkpoint".to_string());
    let placeholder_id = setup_placeholder(&connection, &data_source_id);
    let cancel_token = AtomicBool::new(false);

    let result = replace_placeholder_root_checkpointed(
        &connection,
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
    let row_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
        .expect("count committed rows");
    assert_eq!(row_count, CATALOG_COMMIT_BATCH_ROWS as i64 + 1);
    let unindexed: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM file_entries WHERE partition_index IS NULL",
            [],
            |row| row.get(0),
        )
        .expect("count unindexed rows");
    assert_eq!(unindexed, 0);
}

#[test]
fn batches_catalog_rows_across_directory_boundaries() {
    let connection = persistence_sqlite::open_in_memory().expect("open database");
    persistence_sqlite::runner::run_source_all(&connection).expect("run source migrations");
    let data_source_id = DataSourceId("source-cross-directory-batch".to_string());
    let placeholder_id = setup_placeholder(&connection, &data_source_id);
    let cancel_token = AtomicBool::new(false);

    let result = replace_placeholder_root_checkpointed(
        &connection,
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
    let row_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
        .expect("count committed rows");
    assert_eq!(row_count, CATALOG_COMMIT_BATCH_ROWS as i64 + 1);
}

#[test]
fn completed_checkpointed_enumeration_returns_full_stats() {
    let connection = persistence_sqlite::open_in_memory().expect("open database");
    persistence_sqlite::runner::run_source_all(&connection).expect("run source migrations");
    let data_source_id = DataSourceId("source-complete".to_string());
    let placeholder_id = setup_placeholder(&connection, &data_source_id);

    let stats = replace_placeholder_root_checkpointed(
        &connection,
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
