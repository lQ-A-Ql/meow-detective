use std::{cmp::Ordering, collections::VecDeque, path::Path};

use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::artifact_repo::{
    ArtifactCursorRow, ArtifactRepo, ArtifactSortKey,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use transport::{
    dto::ArtifactRowDto,
    paging::{decode_opaque_cursor, encode_opaque_cursor, PageResponse},
};

use super::super::{source_routing::artifact_to_source_dto, ArtifactServiceError};
use crate::source_db;

mod legacy;

const CURSOR_PAYLOAD_VERSION: u8 = 2;
const CURSOR_KIND: &str = "artifact";
const MAX_CURSOR_SOURCES: usize = 256;
const MAX_CURSOR_VALUE_LENGTH: usize = 4_096;
const MAX_SOURCE_LOOKAHEAD: usize = 32;
const MAX_PAGE_LOOKAHEAD: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArtifactCursorContext {
    family: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArtifactConsumedKey {
    created_at: String,
    artifact_id: String,
}

impl From<&ArtifactSortKey> for ArtifactConsumedKey {
    fn from(key: &ArtifactSortKey) -> Self {
        Self {
            created_at: key.created_at.clone(),
            artifact_id: key.artifact_id.clone(),
        }
    }
}

impl ArtifactConsumedKey {
    fn to_repo_key(&self) -> ArtifactSortKey {
        ArtifactSortKey {
            created_at: self.created_at.clone(),
            artifact_id: self.artifact_id.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArtifactSourceState {
    source_id: String,
    revision: u64,
    snapshot_high_water: i64,
    count: u64,
    consumed: u64,
    after: Option<ArtifactConsumedKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArtifactPageCursor {
    version: u8,
    kind: String,
    case_id: String,
    context: ArtifactCursorContext,
    total: u64,
    sources: Vec<ArtifactSourceState>,
}

struct ArtifactSourcePage {
    state: ArtifactSourceState,
    connection: Connection,
    buffer: VecDeque<ArtifactCursorRow>,
    exhausted: bool,
}

impl ArtifactSourcePage {
    fn ensure_head(
        &mut self,
        family: Option<&str>,
        batch_size: u32,
    ) -> Result<(), ArtifactServiceError> {
        if self.exhausted || !self.buffer.is_empty() {
            return Ok(());
        }
        let after = self
            .state
            .after
            .as_ref()
            .map(ArtifactConsumedKey::to_repo_key);
        let rows = ArtifactRepo::new(&self.connection).list_by_family_after_at_snapshot(
            family,
            after.as_ref(),
            self.state.snapshot_high_water,
            batch_size,
        )?;
        self.exhausted = rows.len() < batch_size as usize;
        self.buffer.extend(rows);
        Ok(())
    }

    fn consume_head(&mut self) -> Result<ArtifactCursorRow, ArtifactServiceError> {
        let row = self.buffer.pop_front().ok_or_else(|| {
            ArtifactServiceError::other("artifact cursor selected a source without a head row")
        })?;
        self.state.after = Some(ArtifactConsumedKey::from(&row.sort_key));
        self.state.consumed = self.state.consumed.saturating_add(1);
        Ok(row)
    }
}

pub fn get_artifact_rows_page_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    family: Option<&str>,
    offset: u64,
    limit: u32,
) -> Result<PageResponse<ArtifactRowDto>, ArtifactServiceError> {
    get_artifact_rows_page_with_cursor_for_case(
        case_conn, case_root, case_id, family, offset, limit, None,
    )
}

pub fn get_artifact_rows_page_with_cursor_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    family: Option<&str>,
    offset: u64,
    limit: u32,
    cursor: Option<&str>,
) -> Result<PageResponse<ArtifactRowDto>, ArtifactServiceError> {
    if cursor.is_some() && offset != 0 {
        return Err(ArtifactServiceError::invalid_input(
            "offset must be zero when cursor is provided",
        ));
    }
    if cursor.is_some() || offset == 0 {
        return query_cursor_page(case_conn, case_root, case_id, family, limit, cursor);
    }
    legacy::query_offset_page(case_conn, case_root, case_id, family, offset, limit)
}

fn query_cursor_page(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    family: Option<&str>,
    limit: u32,
    cursor: Option<&str>,
) -> Result<PageResponse<ArtifactRowDto>, ArtifactServiceError> {
    let context = ArtifactCursorContext {
        family: family.map(str::to_owned),
    };
    let (state, sources) = match cursor {
        Some(cursor) => resume_cursor(case_conn, case_root, case_id, cursor, &context)?,
        None => capture_cursor(case_conn, case_root, case_id, context.clone())?,
    };
    paginate_cursor(state, sources, limit)
}

fn capture_cursor(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    context: ArtifactCursorContext,
) -> Result<(ArtifactPageCursor, Vec<ArtifactSourcePage>), ArtifactServiceError> {
    let mut connections =
        source_db::open_ready_source_connections_read_only(case_conn, case_root, case_id)?;
    connections.sort_by(|left, right| left.0 .0.cmp(&right.0 .0));
    let mut total = 0u64;
    let mut sources = Vec::with_capacity(connections.len());
    for (source_id, connection) in connections {
        let repo = ArtifactRepo::new(&connection);
        let revision = repo.cursor_revision()?;
        let snapshot_high_water = repo.snapshot_high_water()?;
        let count =
            repo.count_for_family_at_snapshot(context.family.as_deref(), snapshot_high_water)?;
        total = total.saturating_add(count);
        sources.push(new_source_page(
            source_id,
            connection,
            revision,
            snapshot_high_water,
            count,
        ));
    }
    let state = ArtifactPageCursor {
        version: CURSOR_PAYLOAD_VERSION,
        kind: CURSOR_KIND.to_string(),
        case_id: case_id.0.clone(),
        context,
        total,
        sources: sources.iter().map(|source| source.state.clone()).collect(),
    };
    Ok((state, sources))
}

fn resume_cursor(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    encoded: &str,
    context: &ArtifactCursorContext,
) -> Result<(ArtifactPageCursor, Vec<ArtifactSourcePage>), ArtifactServiceError> {
    let state: ArtifactPageCursor =
        decode_opaque_cursor(encoded).map_err(|error| invalid_cursor(error.to_string()))?;
    validate_cursor_state(&state, case_id, context)?;
    let mut connections =
        source_db::open_ready_source_connections_read_only(case_conn, case_root, case_id)?;
    connections.sort_by(|left, right| left.0 .0.cmp(&right.0 .0));
    let current_ids = connections
        .iter()
        .map(|(source_id, _)| source_id.0.as_str())
        .collect::<Vec<_>>();
    let cursor_ids = state
        .sources
        .iter()
        .map(|source| source.source_id.as_str())
        .collect::<Vec<_>>();
    if current_ids != cursor_ids {
        return Err(invalid_cursor("data source set changed"));
    }
    let mut sources = Vec::with_capacity(connections.len());
    for ((source_id, connection), source_state) in
        connections.into_iter().zip(state.sources.iter().cloned())
    {
        let repo = ArtifactRepo::new(&connection);
        if repo.cursor_revision()? != source_state.revision {
            return Err(invalid_cursor(format!(
                "artifact snapshot changed for source {}",
                source_id.0
            )));
        }
        let current_count = repo.count_for_family_at_snapshot(
            context.family.as_deref(),
            source_state.snapshot_high_water,
        )?;
        if current_count != source_state.count {
            return Err(invalid_cursor(format!(
                "artifact snapshot changed for source {}",
                source_id.0
            )));
        }
        sources.push(ArtifactSourcePage {
            exhausted: source_state.consumed >= source_state.count,
            state: source_state,
            connection,
            buffer: VecDeque::new(),
        });
    }
    Ok((state, sources))
}

fn paginate_cursor(
    mut state: ArtifactPageCursor,
    mut sources: Vec<ArtifactSourcePage>,
    limit: u32,
) -> Result<PageResponse<ArtifactRowDto>, ArtifactServiceError> {
    let batch_size = cursor_batch_size(limit, sources.len());
    let mut items = Vec::with_capacity(limit as usize);
    validate_artifact_revisions(&sources)?;
    while items.len() < limit as usize {
        fill_artifact_heads(&mut sources, state.context.family.as_deref(), batch_size)?;
        validate_artifact_snapshot(&sources)?;
        let Some(source_index) = next_artifact_source(&sources) else {
            break;
        };
        let row = sources[source_index].consume_head()?;
        items.push(artifact_to_source_dto(
            &row.artifact,
            &DataSourceId(sources[source_index].state.source_id.clone()),
        ));
    }
    fill_artifact_heads(&mut sources, state.context.family.as_deref(), batch_size)?;
    validate_artifact_snapshot(&sources)?;
    validate_artifact_revisions(&sources)?;
    let has_more = sources.iter().any(|source| !source.buffer.is_empty());
    state.sources = sources.into_iter().map(|source| source.state).collect();
    let next_cursor = has_more
        .then(|| encode_opaque_cursor(&state))
        .transpose()
        .map_err(|error| ArtifactServiceError::other(error.to_string()))?;
    Ok(PageResponse {
        total: state.total,
        items,
        next_cursor,
    })
}

fn validate_artifact_revisions(sources: &[ArtifactSourcePage]) -> Result<(), ArtifactServiceError> {
    for source in sources {
        if ArtifactRepo::new(&source.connection).cursor_revision()? != source.state.revision {
            return Err(invalid_cursor(format!(
                "artifact snapshot changed for source {}",
                source.state.source_id
            )));
        }
    }
    Ok(())
}

fn fill_artifact_heads(
    sources: &mut [ArtifactSourcePage],
    family: Option<&str>,
    batch_size: u32,
) -> Result<(), ArtifactServiceError> {
    for source in sources {
        source.ensure_head(family, batch_size)?;
    }
    Ok(())
}

fn validate_artifact_snapshot(sources: &[ArtifactSourcePage]) -> Result<(), ArtifactServiceError> {
    for source in sources {
        let visible = source
            .state
            .consumed
            .saturating_add(source.buffer.len() as u64);
        if visible > source.state.count || (source.exhausted && visible != source.state.count) {
            return Err(invalid_cursor(format!(
                "artifact snapshot changed for source {}",
                source.state.source_id
            )));
        }
    }
    Ok(())
}

fn next_artifact_source(sources: &[ArtifactSourcePage]) -> Option<usize> {
    sources
        .iter()
        .enumerate()
        .filter_map(|(index, source)| source.buffer.front().map(|head| (index, source, head)))
        .min_by(|(_, left_source, left), (_, right_source, right)| {
            compare_artifact_rows(left_source, left, right_source, right)
        })
        .map(|(index, _, _)| index)
}

fn compare_artifact_rows(
    left_source: &ArtifactSourcePage,
    left: &ArtifactCursorRow,
    right_source: &ArtifactSourcePage,
    right: &ArtifactCursorRow,
) -> Ordering {
    right
        .sort_key
        .created_at
        .cmp(&left.sort_key.created_at)
        .then_with(|| {
            left_source
                .state
                .source_id
                .cmp(&right_source.state.source_id)
        })
        .then_with(|| left.sort_key.artifact_id.cmp(&right.sort_key.artifact_id))
}

fn new_source_page(
    source_id: DataSourceId,
    connection: Connection,
    revision: u64,
    snapshot_high_water: i64,
    count: u64,
) -> ArtifactSourcePage {
    ArtifactSourcePage {
        state: ArtifactSourceState {
            source_id: source_id.0,
            revision,
            snapshot_high_water,
            count,
            consumed: 0,
            after: None,
        },
        connection,
        buffer: VecDeque::new(),
        exhausted: count == 0,
    }
}

fn cursor_batch_size(limit: u32, source_count: usize) -> u32 {
    if source_count == 0 {
        return 1;
    }
    let budget = (limit as usize).clamp(1, MAX_PAGE_LOOKAHEAD);
    budget.div_ceil(source_count).clamp(1, MAX_SOURCE_LOOKAHEAD) as u32
}

fn validate_cursor_state(
    state: &ArtifactPageCursor,
    case_id: &CaseId,
    context: &ArtifactCursorContext,
) -> Result<(), ArtifactServiceError> {
    if state.version != CURSOR_PAYLOAD_VERSION
        || state.kind != CURSOR_KIND
        || state.case_id != case_id.0
        || &state.context != context
    {
        return Err(invalid_cursor("query context does not match"));
    }
    if state.sources.len() > MAX_CURSOR_SOURCES {
        return Err(invalid_cursor("source count exceeds the supported bound"));
    }
    let mut previous_source = None;
    let mut total = 0u64;
    for source in &state.sources {
        validate_source_state(source, previous_source)?;
        previous_source = Some(source.source_id.as_str());
        total = total.saturating_add(source.count);
    }
    if total != state.total {
        return Err(invalid_cursor("snapshot total is inconsistent"));
    }
    Ok(())
}

fn validate_source_state(
    source: &ArtifactSourceState,
    previous_source: Option<&str>,
) -> Result<(), ArtifactServiceError> {
    if source.source_id.is_empty()
        || source.source_id.len() > MAX_CURSOR_VALUE_LENGTH
        || source.snapshot_high_water < 0
        || source.consumed > source.count
        || previous_source.is_some_and(|previous| previous >= source.source_id.as_str())
        || (source.consumed == 0) != source.after.is_none()
    {
        return Err(invalid_cursor("source snapshot state is invalid"));
    }
    if source.after.as_ref().is_some_and(|after| {
        after.created_at.is_empty()
            || after.created_at.len() > MAX_CURSOR_VALUE_LENGTH
            || after.artifact_id.is_empty()
            || after.artifact_id.len() > MAX_CURSOR_VALUE_LENGTH
    }) {
        return Err(invalid_cursor("consumed sort key is invalid"));
    }
    Ok(())
}

fn invalid_cursor(reason: impl AsRef<str>) -> ArtifactServiceError {
    ArtifactServiceError::invalid_input(format!(
        "invalid or stale artifact cursor: {}",
        reason.as_ref()
    ))
}
