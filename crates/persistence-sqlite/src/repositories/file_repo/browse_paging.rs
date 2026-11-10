use super::FileRepo;
use crate::connection::DbResult;
use domain::{FileEntry, FileEntryId};

const MAX_SORTABLE_CHILD_ENTRIES: u64 = 8_192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSortKey {
    Name,
    Size,
    ModifiedAt,
    Ext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSortDirection {
    Asc,
    Desc,
}

impl FileRepo<'_> {
    pub fn count_entries_visible(
        &self,
        parent_id: Option<&FileEntryId>,
        show_hidden: bool,
    ) -> DbResult<u64> {
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            let owner = crate::source_fjall::source_id(self.conn)?
                .ok_or_else(|| crate::DbError::System("source identity is missing".into()))?;
            let store = crate::source_fjall::require_source_store(self.conn)?;
            let catalog =
                persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
            let snapshot = catalog.read_snapshot();
            let mut cursor = None;
            let mut count = 0u64;
            loop {
                crate::read_operation::ensure_active()?;
                let page = catalog
                    .list_by_parent(
                        &snapshot,
                        &owner,
                        parent_id.map(|id| id.0.as_str()),
                        cursor.as_deref(),
                        persistence_fjall::ScanDirection::Forward,
                        512,
                    )
                    .map_err(|error| crate::DbError::System(error.to_string()))?;
                count = count.saturating_add(
                    page.records
                        .iter()
                        .filter(|record| {
                            !record.deleted && (show_hidden || (!record.hidden && !record.system))
                        })
                        .count() as u64,
                );
                cursor = page.next_cursor;
                if cursor.is_none() {
                    return Ok(count);
                }
            }
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn entries_need_natural_sort(
        &self,
        parent_id: Option<&FileEntryId>,
        show_hidden: bool,
        sort_key: FileSortKey,
    ) -> DbResult<bool> {
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            match sort_key {
                FileSortKey::Size | FileSortKey::ModifiedAt => return Ok(false),
                FileSortKey::Name | FileSortKey::Ext => {}
            }
            let count = self.count_entries_visible(parent_id, show_hidden)?;
            let mut offset = 0u64;
            while offset < count {
                let entries = self
                    .fjall_child_page(
                        None,
                        parent_id.map(|id| id.0.as_str()),
                        offset,
                        512,
                        false,
                        !show_hidden,
                    )?
                    .unwrap_or_default();
                if entries.is_empty() {
                    return Err(crate::DbError::System(
                        "catalog child count changed during natural sort check".into(),
                    ));
                }
                offset = offset.saturating_add(entries.len() as u64);
                if entries.iter().any(|entry| match sort_key {
                    FileSortKey::Ext => {
                        contains_ascii_digit(&entry.name)
                            || entry.ext.as_deref().is_some_and(contains_ascii_digit)
                    }
                    FileSortKey::Name => contains_ascii_digit(&entry.name),
                    FileSortKey::Size | FileSortKey::ModifiedAt => false,
                }) {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }

    pub fn find_entries_page_visible_ordered(
        &self,
        parent_id: Option<&FileEntryId>,
        offset: u64,
        limit: u32,
        show_hidden: bool,
        sort_key: FileSortKey,
        sort_direction: FileSortDirection,
    ) -> DbResult<Vec<FileEntry>> {
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            let total = self.count_entries_visible(parent_id, show_hidden)?;
            let window = offset.checked_add(u64::from(limit)).ok_or_else(|| {
                crate::DbError::System("file page window exceeds the bounded catalog range".into())
            })?;
            if total > MAX_SORTABLE_CHILD_ENTRIES || window > MAX_SORTABLE_CHILD_ENTRIES {
                return Err(crate::DbError::System(format!(
                    "ordered catalog page exceeds the bounded limit of {MAX_SORTABLE_CHILD_ENTRIES} rows"
                )));
            }
            let mut entries = self
                .fjall_child_page(
                    None,
                    parent_id.map(|id| id.0.as_str()),
                    0,
                    u32::try_from(total).unwrap_or(0),
                    false,
                    !show_hidden,
                )?
                .unwrap_or_default();
            sort_native_entries(&mut entries, sort_key, sort_direction);
            return Ok(entries
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect());
        }
        Err(crate::DbError::System("source identity is missing".into()))
    }
}

fn contains_ascii_digit(value: &str) -> bool {
    value.bytes().any(|byte| byte.is_ascii_digit())
}

fn sort_native_entries(
    entries: &mut [FileEntry],
    sort_key: FileSortKey,
    sort_direction: FileSortDirection,
) {
    entries.sort_by(|left, right| {
        right
            .is_directory()
            .cmp(&left.is_directory())
            .then_with(|| visibility_rank(left).cmp(&visibility_rank(right)))
            .then_with(|| {
                let ordering = match sort_key {
                    FileSortKey::Name => left
                        .name
                        .to_ascii_lowercase()
                        .cmp(&right.name.to_ascii_lowercase()),
                    FileSortKey::Size => left
                        .size
                        .unwrap_or_default()
                        .cmp(&right.size.unwrap_or_default()),
                    FileSortKey::ModifiedAt => left.modified_at.cmp(&right.modified_at),
                    FileSortKey::Ext => left
                        .ext
                        .as_deref()
                        .unwrap_or_default()
                        .to_ascii_lowercase()
                        .cmp(
                            &right
                                .ext
                                .as_deref()
                                .unwrap_or_default()
                                .to_ascii_lowercase(),
                        ),
                };
                match sort_direction {
                    FileSortDirection::Asc => ordering,
                    FileSortDirection::Desc => ordering.reverse(),
                }
            })
            .then_with(|| {
                left.name
                    .to_ascii_lowercase()
                    .cmp(&right.name.to_ascii_lowercase())
            })
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.0.cmp(&right.id.0))
    });
}

fn visibility_rank(entry: &FileEntry) -> u8 {
    match (entry.deleted, entry.hidden || entry.system) {
        (false, false) => 0,
        (false, true) => 1,
        (true, false) => 2,
        (true, true) => 3,
    }
}
