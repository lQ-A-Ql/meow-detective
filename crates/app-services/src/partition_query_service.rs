use domain::DataSourceId;
pub use persistence_fjall::PartitionRecord;
use persistence_sqlite::{
    repositories::storage_control_repo::{StorageControlRepo, StorageOperationState},
    DbError, DbResult,
};
use rusqlite::{Connection, OptionalExtension};
use std::path::{Path, PathBuf};

pub struct PartitionQueryRepo<'a> {
    connection: &'a Connection,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourcePartitionIdentities {
    pub gpt_disk_guid: Option<String>,
}

impl<'a> PartitionQueryRepo<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn find_by_data_source(&self, data_source_id: &str) -> DbResult<Vec<PartitionRecord>> {
        projected_partitions(self.connection, data_source_id)
    }

    pub fn find_by_data_source_and_index(
        &self,
        data_source_id: &str,
        partition_index: usize,
    ) -> DbResult<Option<PartitionRecord>> {
        Ok(projected_partitions(self.connection, data_source_id)?
            .into_iter()
            .find(|record| record.partition_index as usize == partition_index))
    }

    pub fn count_by_data_source(&self, data_source_id: &str) -> DbResult<u64> {
        Ok(projected_partitions(self.connection, data_source_id)?.len() as u64)
    }

    pub fn source_records(&self, data_source_id: &str) -> DbResult<Vec<PartitionRecord>> {
        partition_records(self.connection, data_source_id)?.ok_or_else(|| {
            DbError::System(format!(
                "Fjall partition records are unavailable for data source '{data_source_id}'"
            ))
        })
    }

    pub fn identities_for_data_source(
        &self,
        data_source_id: &str,
    ) -> DbResult<SourcePartitionIdentities> {
        Ok(SourcePartitionIdentities {
            gpt_disk_guid: self
                .find_by_data_source(data_source_id)?
                .into_iter()
                .find_map(|partition| partition.gpt_disk_guid),
        })
    }

    pub fn mark_catalog_ready(
        &self,
        data_source_id: &str,
        partition_index: u32,
        filesystem: &str,
    ) -> DbResult<bool> {
        let case_root = case_root_from_connection(self.connection)?.ok_or_else(|| {
            DbError::System("source database path does not identify a case root".to_owned())
        })?;
        let source = crate::source_db::open_registered_fjall_source_read_write(
            &case_root,
            &DataSourceId(data_source_id.to_owned()),
        )
        .map_err(storage_error)?;
        source
            .partition_mark_catalog_ready(
                data_source_id,
                partition_index,
                filesystem,
                persistence_fjall::PersistMode::SyncAll,
            )
            .map_err(storage_error)
    }
}

fn projected_partitions(
    connection: &Connection,
    source_id: &str,
) -> DbResult<Vec<PartitionRecord>> {
    published_partitions(connection, source_id)?.ok_or_else(|| {
        DbError::System(format!(
            "Fjall partition records are unavailable for data source '{source_id}'"
        ))
    })
}

fn published_partitions(
    connection: &Connection,
    source_id: &str,
) -> DbResult<Option<Vec<PartitionRecord>>> {
    partition_records(connection, source_id)
}

fn partition_records(
    connection: &Connection,
    source_id: &str,
) -> DbResult<Option<Vec<PartitionRecord>>> {
    let Some(case_root) = case_root_from_connection(connection)? else {
        return Ok(None);
    };
    let source_id = DataSourceId(source_id.to_owned());
    if let Some(build_root) = active_import_storage_root(connection, &case_root, &source_id)? {
        if !build_root.is_dir() {
            return Err(DbError::System(format!(
                "active Fjall import build is missing for data source '{}'",
                source_id.0
            )));
        }
        let source = crate::storage_runtime::ReadOnlyStorageRuntime::open(&build_root)
            .map_err(storage_error)?
            .source();
        let snapshot = source.snapshot();
        return read_partition_pages(&source, &snapshot, &source_id.0).map(Some);
    }
    let bound_root = persistence_sqlite::source_fjall::open_store(connection)?
        .map(|store| store.database().path().to_path_buf());
    if let Some(bound_root) = bound_root.filter(|path| path.is_dir()) {
        let source = crate::storage_runtime::ReadOnlyStorageRuntime::open(&bound_root)
            .map_err(storage_error)?
            .source();
        let snapshot = source.snapshot();
        if source
            .partition_projection_complete(&snapshot, &source_id.0)
            .map_err(storage_error)?
        {
            return read_partition_pages(&source, &snapshot, &source_id.0).map(Some);
        }
    }
    let storage_root = crate::source_db::fjall_source_storage_root(&case_root, &source_id.0)
        .map_err(storage_error)?;
    if storage_root.is_dir() {
        let source =
            crate::source_db::open_registered_fjall_source_read_only(&case_root, &source_id)
                .map_err(storage_error)?;
        let snapshot = source.snapshot();
        if source
            .source_projection_complete(&snapshot, &source_id.0)
            .map_err(storage_error)?
            || source
                .partition_projection_complete(&snapshot, &source_id.0)
                .map_err(storage_error)?
        {
            return read_partition_pages(&source, &snapshot, &source_id.0).map(Some);
        }
    }

    Ok(None)
}

fn read_partition_pages(
    source: &crate::storage_runtime::ReadOnlySourceStorageHandle,
    snapshot: &persistence_fjall::SourceReadSnapshot,
    source_id: &str,
) -> DbResult<Vec<PartitionRecord>> {
    let mut cursor = None;
    let mut records = Vec::new();
    loop {
        let page = source
            .partition_list_by_source(
                snapshot,
                source_id,
                cursor.as_deref(),
                persistence_fjall::ScanDirection::Forward,
                256,
            )
            .map_err(storage_error)?;
        records.extend(page.records);
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(records);
        }
    }
}

fn active_import_storage_root(
    source_connection: &Connection,
    case_root: &Path,
    source_id: &DataSourceId,
) -> DbResult<Option<PathBuf>> {
    let Some(case_id) = source_connection
        .query_row(
            "SELECT case_id FROM data_sources WHERE id = ?1",
            [&source_id.0],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    else {
        return Ok(None);
    };
    let case_database = case_root.join("app.db");
    if !case_database.is_file() {
        return Ok(None);
    }
    let case_connection = crate::connection::open_existing_case_db(&case_database)?;
    let mut active = StorageControlRepo::new(&case_connection)
        .list_open_operations(&case_id)?
        .into_iter()
        .filter(|operation| {
            operation.source_id.as_deref() == Some(source_id.0.as_str())
                && operation.operation_kind == "source_import"
                && matches!(
                    operation.state,
                    StorageOperationState::Planned | StorageOperationState::Running
                )
        });
    let Some(operation) = active.next() else {
        return Ok(None);
    };
    if active.next().is_some() {
        return Err(DbError::System(format!(
            "multiple source imports are active for data source '{}'",
            source_id.0
        )));
    }
    let layout = persistence_fjall::SourceBuildLayout::for_case_source(
        case_root,
        &source_id.0,
        &operation.attempt_id,
    )
    .map_err(|error| DbError::System(error.to_string()))?;
    Ok(Some(layout.build_path()))
}

fn case_root_from_connection(connection: &Connection) -> DbResult<Option<PathBuf>> {
    let database_path: String =
        connection.query_row("PRAGMA database_list", [], |row| row.get(2))?;
    let path = Path::new(&database_path);
    let root = match path.file_name().and_then(|name| name.to_str()) {
        Some(name) if name.eq_ignore_ascii_case("app.db") => path.parent(),
        Some(name) if name.eq_ignore_ascii_case("source.db") => {
            path.parent().and_then(Path::parent).and_then(Path::parent)
        }
        _ => None,
    };
    Ok(root.map(Path::to_path_buf))
}

fn storage_error(error: crate::source_db::FjallStorageRuntimeError) -> DbError {
    DbError::System(error.to_string())
}
