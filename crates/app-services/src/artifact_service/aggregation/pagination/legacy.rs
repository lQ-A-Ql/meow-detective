use std::{cmp::Ordering, collections::VecDeque, path::Path};

use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::artifact_repo::{ArtifactRepo, ArtifactSortKey};
use rusqlite::Connection;
use transport::{dto::ArtifactRowDto, paging::PageResponse};

use super::super::super::{source_routing::artifact_to_source_dto, ArtifactServiceError};
use crate::source_db;

const ARTIFACT_MERGE_BATCH_SIZE: u32 = 256;

pub(super) fn query_offset_page(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    family: Option<&str>,
    offset: u64,
    limit: u32,
) -> Result<PageResponse<ArtifactRowDto>, ArtifactServiceError> {
    let mut total = 0u64;
    let mut sources = Vec::new();
    for (source_id, source_conn) in
        source_db::open_ready_source_connections_read_only(case_conn, case_root, case_id)?
    {
        total = total.saturating_add(ArtifactRepo::new(&source_conn).count_for_family(family)?);
        sources.push(LegacyArtifactSource::new(source_id, source_conn));
    }
    if limit == 0 || offset >= total {
        return Ok(PageResponse {
            total,
            items: Vec::new(),
            next_cursor: None,
        });
    }
    merge_offset_page(sources, family, offset, limit, total)
}

fn merge_offset_page(
    mut sources: Vec<LegacyArtifactSource>,
    family: Option<&str>,
    offset: u64,
    limit: u32,
    total: u64,
) -> Result<PageResponse<ArtifactRowDto>, ArtifactServiceError> {
    let scan_end = offset.saturating_add(u64::from(limit)).min(total);
    let batch_size = limit.max(ARTIFACT_MERGE_BATCH_SIZE);
    let mut position = 0u64;
    let mut items = Vec::with_capacity(limit as usize);
    while position < scan_end {
        for source in &mut sources {
            source.refill(family, batch_size)?;
        }
        let Some(source_index) = next_source(&sources) else {
            break;
        };
        let row = sources[source_index].buffer.pop_front().ok_or_else(|| {
            ArtifactServiceError::other(
                "artifact merge cursor selected a source without a buffered row",
            )
        })?;
        if position >= offset {
            items.push(row);
        }
        position = position.saturating_add(1);
    }
    Ok(PageResponse {
        total,
        items,
        next_cursor: None,
    })
}

struct LegacyArtifactSource {
    source_id: DataSourceId,
    connection: Connection,
    after: Option<ArtifactSortKey>,
    buffer: VecDeque<ArtifactRowDto>,
    exhausted: bool,
}

impl LegacyArtifactSource {
    fn new(source_id: DataSourceId, connection: Connection) -> Self {
        Self {
            source_id,
            connection,
            after: None,
            buffer: VecDeque::new(),
            exhausted: false,
        }
    }

    fn refill(
        &mut self,
        family: Option<&str>,
        batch_size: u32,
    ) -> Result<(), ArtifactServiceError> {
        if self.exhausted || !self.buffer.is_empty() {
            return Ok(());
        }
        let artifacts = ArtifactRepo::new(&self.connection).list_by_family_after(
            family,
            self.after.as_ref(),
            batch_size,
        )?;
        self.exhausted = artifacts.len() < batch_size as usize;
        self.after = artifacts.last().map(|row| row.sort_key.clone());
        self.buffer.extend(
            artifacts
                .iter()
                .map(|row| artifact_to_source_dto(&row.artifact, &self.source_id)),
        );
        Ok(())
    }
}

fn next_source(sources: &[LegacyArtifactSource]) -> Option<usize> {
    sources
        .iter()
        .enumerate()
        .filter_map(|(index, source)| source.buffer.front().map(|row| (index, row)))
        .min_by(|(_, left), (_, right)| compare_rows(left, right))
        .map(|(index, _)| index)
}

fn compare_rows(left: &ArtifactRowDto, right: &ArtifactRowDto) -> Ordering {
    right
        .created_at
        .cmp(&left.created_at)
        .then_with(|| left.id.cmp(&right.id))
}
