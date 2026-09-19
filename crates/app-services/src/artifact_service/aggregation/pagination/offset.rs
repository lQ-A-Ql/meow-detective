use std::path::Path;

use domain::CaseId;
use rusqlite::Connection;
use transport::{dto::ArtifactRowDto, paging::PageResponse};

use super::{query_cursor_page, ArtifactServiceError, MAX_PAGE_LOOKAHEAD};

pub(super) fn query_page(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    family: Option<&str>,
    offset: u64,
    limit: u32,
) -> Result<PageResponse<ArtifactRowDto>, ArtifactServiceError> {
    let initial = query_cursor_page(case_conn, case_root, case_id, family, 0, None)?;
    if limit == 0 || offset >= initial.total {
        return Ok(PageResponse {
            total: initial.total,
            items: Vec::new(),
            next_cursor: None,
        });
    }

    let mut remaining = offset;
    let mut cursor = initial.next_cursor;
    while remaining > 0 {
        let skip_limit = remaining.min(MAX_PAGE_LOOKAHEAD as u64) as u32;
        let page = query_cursor_page(
            case_conn,
            case_root,
            case_id,
            family,
            skip_limit,
            cursor.as_deref(),
        )?;
        let consumed = page.items.len() as u64;
        remaining = remaining.saturating_sub(consumed);
        cursor = page.next_cursor;
        if consumed < u64::from(skip_limit) {
            return Ok(empty_page(initial.total));
        }
        if remaining > 0 && cursor.is_none() {
            return Ok(empty_page(initial.total));
        }
    }

    if cursor.is_none() {
        return Ok(empty_page(initial.total));
    }
    let page = query_cursor_page(
        case_conn,
        case_root,
        case_id,
        family,
        limit,
        cursor.as_deref(),
    )?;
    Ok(PageResponse {
        total: initial.total,
        items: page.items,
        next_cursor: None,
    })
}

fn empty_page(total: u64) -> PageResponse<ArtifactRowDto> {
    PageResponse {
        total,
        items: Vec::new(),
        next_cursor: None,
    }
}
