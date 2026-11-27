use domain::DataSource;
use persistence_source::fjall::{CatalogEntryType, CatalogRecordStore, FileEntryRecord};

use super::{CatalogAccumulator, CatalogRow};
use crate::derived_source_service::{
    DerivedSourceError, DerivedSourceResult, MaterializedRbdSource,
};

const PAGE_ROWS: usize = 512;

pub(super) fn summarize(
    connection: &rusqlite::Connection,
    data_source: DataSource,
) -> DerivedSourceResult<MaterializedRbdSource> {
    let owner = persistence_source::detail::source_id(connection)?;
    if owner.as_deref() != Some(data_source.id.0.as_str()) {
        return Err(DerivedSourceError::InconsistentState(
            "catalog summary source owner mismatch".into(),
        ));
    }
    let store = persistence_source::detail::require_source_store(connection)?;
    let catalog = CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = catalog.read_snapshot();
    let mut after = None;
    let mut accumulator = CatalogAccumulator::default();
    loop {
        persistence_sqlite::read_operation::ensure_active()?;
        let page = catalog
            .list_by_id(&snapshot, &data_source.id.0, after.as_deref(), PAGE_ROWS)
            .map_err(storage_error)?;
        if page.records.is_empty() && page.next_cursor.is_some() {
            return Err(DerivedSourceError::InconsistentState(
                "catalog summary cursor did not advance".into(),
            ));
        }
        for entry in page.records {
            if after
                .as_ref()
                .is_some_and(|previous: &String| previous >= &entry.id)
            {
                return Err(DerivedSourceError::InconsistentState(
                    "catalog summary id order is invalid".into(),
                ));
            }
            after = Some(entry.id.clone());
            let parent = entry
                .parent_id
                .as_deref()
                .map(|parent| catalog.get(&snapshot, &data_source.id.0, parent))
                .transpose()
                .map_err(storage_error)?
                .flatten();
            accumulator.push(row(entry, parent));
        }
        if page.next_cursor.is_none() {
            return Ok(accumulator.into_summary(data_source));
        }
    }
}

fn row(entry: FileEntryRecord, parent: Option<FileEntryRecord>) -> CatalogRow {
    let parent_state = if entry.parent_id.is_none() {
        0
    } else if parent.is_some() {
        1
    } else {
        2
    };
    CatalogRow {
        id: entry.id,
        parent_id: entry.parent_id,
        path: entry.path,
        name: entry.name,
        entry_type: entry_kind(entry.entry_type).into(),
        size: entry.size,
        deleted: i64::from(entry.deleted),
        hidden: i64::from(entry.hidden),
        system: i64::from(entry.system),
        created_at: entry.created.map(|value| value.to_rfc3339()),
        modified_at: entry.modified.map(|value| value.to_rfc3339()),
        accessed_at: entry.accessed.map(|value| value.to_rfc3339()),
        changed_at: entry.changed.map(|value| value.to_rfc3339()),
        parent_state,
        parent_path: parent.as_ref().map(|value| value.path.clone()),
        parent_name: parent.as_ref().map(|value| value.name.clone()),
        parent_entry_type: parent.map(|value| entry_kind(value.entry_type).into()),
    }
}

fn entry_kind(kind: CatalogEntryType) -> &'static str {
    match kind {
        CatalogEntryType::File => "file",
        CatalogEntryType::Directory => "directory",
    }
}

fn storage_error(error: persistence_source::fjall::FjallStorageError) -> DerivedSourceError {
    DerivedSourceError::Database(persistence_sqlite::DbError::System(error.to_string()))
}
