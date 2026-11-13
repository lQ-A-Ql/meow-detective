use rusqlite::Connection;
use std::path::Path;
use transport::{commands::SearchFilesRequest, dto::SearchFileResultPageDto};

use super::{
    bounded_scan_end, buffer::SourceBuffer, current_sources, merge::MergeHeap, MAX_OFFSET_WINDOW,
};
use crate::search_service::SearchError;

pub(super) fn query(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    request: &SearchFilesRequest,
) -> Result<SearchFileResultPageDto, SearchError> {
    if request.limit > 0 && request.offset >= MAX_OFFSET_WINDOW {
        return Err(SearchError::InvalidInput(format!(
            "offset {0} 超过无游标分页窗口，请使用 nextCursor 继续",
            request.offset
        )));
    }
    let start = std::time::Instant::now();
    let source_set = current_sources(case_conn, case_root, case_id, request)?;
    let mut sources = source_set
        .sources
        .into_iter()
        .map(|source| SourceBuffer::new(source, None, None, request.limit.min(64) as usize))
        .collect::<Result<Vec<_>, _>>()?;
    let total = sources
        .iter()
        .fold(0u64, |total, source| total.saturating_add(source.total));
    let available = total.min(MAX_OFFSET_WINDOW);
    let mut heap = MergeHeap::new(super::sort_direction(request.sort_direction));
    for (index, source) in sources.iter().enumerate() {
        if let Some((sort, id)) = source.front_key() {
            heap.push(index, sort, id);
        }
    }
    let mut items = Vec::with_capacity(request.limit as usize);
    let merge_end = (bounded_scan_end(request.offset, request.limit) as u64).min(available);
    for position in 0..merge_end {
        let Some(index) = heap.pop() else {
            break;
        };
        if let Some(item) = sources[index].take(position >= request.offset)? {
            items.push(item);
        }
        if position + 1 == merge_end {
            break;
        }
        sources[index].refill()?;
        if let Some((sort, id)) = sources[index].front_key() {
            heap.push(index, sort, id);
        }
    }
    Ok(SearchFileResultPageDto {
        total,
        available,
        truncated: available < total,
        took_ms: start.elapsed().as_millis() as u64,
        items,
        coverage: source_set.coverage,
        next_cursor: None,
    })
}
