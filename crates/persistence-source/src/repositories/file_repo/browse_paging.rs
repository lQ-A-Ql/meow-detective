use super::FileRepo;
use domain::{FileEntry, FileEntryId};
use persistence_fjall::{CatalogBrowsePage, CatalogBrowseQuery, CatalogRecordStore, CatalogSort};
use persistence_sqlite::{DbError, DbResult};

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
        Ok(self
            .indexed_page(
                parent_id,
                0,
                0,
                show_hidden,
                Some((FileSortKey::Name, FileSortDirection::Asc)),
            )?
            .total)
    }

    pub fn find_entries_page_visible_ordered(
        &self,
        parent_id: Option<&FileEntryId>,
        offset: u64,
        limit: u32,
        show_hidden: bool,
        sort: FileSortKey,
        direction: FileSortDirection,
    ) -> DbResult<Vec<FileEntry>> {
        Ok(self
            .indexed_page(
                parent_id,
                offset,
                limit,
                show_hidden,
                Some((sort, direction)),
            )?
            .records
            .into_iter()
            .map(crate::detail::file_entry)
            .collect())
    }

    pub(super) fn indexed_page(
        &self,
        parent_id: Option<&FileEntryId>,
        offset: u64,
        limit: u32,
        show_hidden: bool,
        order: Option<(FileSortKey, FileSortDirection)>,
    ) -> DbResult<CatalogBrowsePage> {
        persistence_sqlite::read_operation::ensure_active()?;
        let owner = crate::detail::source_id(self.conn)?
            .ok_or_else(|| DbError::System("source identity is missing".into()))?;
        let store = crate::detail::require_source_store(self.conn)?;
        let catalog = CatalogRecordStore::from_source_store((*store).clone());
        let directories_only = order.is_none();
        let (sort, direction) = order.unwrap_or((FileSortKey::Name, FileSortDirection::Asc));
        let sort = match sort {
            FileSortKey::Name => CatalogSort::Name,
            FileSortKey::Size => CatalogSort::Size,
            FileSortKey::ModifiedAt => CatalogSort::Modified,
            FileSortKey::Ext => CatalogSort::Extension,
        };
        catalog
            .browse(
                &catalog.read_snapshot(),
                &CatalogBrowseQuery {
                    source: &owner,
                    parent: parent_id.map(|id| id.0.as_str()),
                    directories_only,
                    show_hidden,
                    sort,
                    descending: direction == FileSortDirection::Desc,
                    offset: usize::try_from(offset)
                        .map_err(|_| DbError::System("catalog offset is too large".into()))?,
                    limit: limit as usize,
                },
            )
            .map_err(|error| DbError::System(error.to_string()))
    }
}
