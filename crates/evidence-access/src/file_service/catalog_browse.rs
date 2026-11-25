use super::{fjall_read::record_to_domain, FileServiceError};
use domain::FileEntry;
use persistence_source::{
    fjall::{CatalogBrowseQuery, CatalogSort},
    runtime::ReadOnlySourceStorageHandle,
};
use transport::commands::{FileSortDirectionDto, FileSortKeyDto, GetFileRowsRequest};

pub(super) fn page(
    source: &ReadOnlySourceStorageHandle,
    source_id: &str,
    request: &GetFileRowsRequest,
    directories_only: bool,
) -> Result<(Vec<FileEntry>, u64), FileServiceError> {
    let sort = match request.sort_key {
        FileSortKeyDto::Name => CatalogSort::Name,
        FileSortKeyDto::Size => CatalogSort::Size,
        FileSortKeyDto::ModifiedAt => CatalogSort::Modified,
        FileSortKeyDto::Ext => CatalogSort::Extension,
    };
    let snapshot = source.snapshot();
    let page = source
        .catalog_browse(
            &snapshot,
            &CatalogBrowseQuery {
                source: source_id,
                parent: request.parent_id.as_deref(),
                directories_only,
                show_hidden: request.show_hidden,
                sort,
                descending: request.sort_direction == FileSortDirectionDto::Desc,
                offset: usize::try_from(request.offset)
                    .map_err(|_| FileServiceError::invalid_input("catalog offset is too large"))?,
                limit: request.limit as usize,
            },
        )
        .map_err(|error| FileServiceError::other(error.to_string()))?;
    Ok((
        page.records.into_iter().map(record_to_domain).collect(),
        page.total,
    ))
}
