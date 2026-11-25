use super::{mapping, RegistryBrowserContext, RegistryBrowserError, RegistryBrowserRegistry};
use domain::DataSourceId;
use persistence_source::repositories::{artifact_repo::ArtifactRepo, file_repo::FileRepo};
use transport::dto::{RegistryHiveListRequestDto, RegistryHivePageDto};

pub fn list_hives(
    registry: &RegistryBrowserRegistry,
    ctx: &RegistryBrowserContext<'_>,
    request: RegistryHiveListRequestDto,
) -> Result<RegistryHivePageDto, RegistryBrowserError> {
    let limit = mapping::page_limit(request.limit)?;
    if request.data_source_id.len() > 256 {
        return Err(RegistryBrowserError::InvalidRequest);
    }
    let source = DataSourceId(request.data_source_id);
    let conn = ctx.source(&source)?;
    let revision = ctx.revision(&source)?;
    let repo = FileRepo::new(&conn);
    let mut entries = hive_entries(&conn, &repo, &source)?;
    let total_count = entries.len() as u64;
    let after = if let Some(id) = request.cursor {
        let state = registry.lock()?;
        let cursor = state
            .discovery_cursors
            .get(&id)
            .ok_or(RegistryBrowserError::InvalidRequest)?;
        if cursor.case_id != ctx.case_id.0
            || cursor.source_id != source.0
            || cursor.revision != revision
        {
            return Err(RegistryBrowserError::InvalidRequest);
        }
        cursor.after.clone()
    } else {
        String::new()
    };
    if !after.is_empty() {
        let Some(position) = entries.iter().position(|entry| entry.id.0 == after) else {
            return Err(RegistryBrowserError::InvalidRequest);
        };
        entries.drain(..=position);
    }
    let page_count = entries.len().min(limit);
    let hives = entries
        .iter()
        .take(page_count)
        .map(mapping::descriptor)
        .collect::<Vec<_>>();
    let more = entries.len() > limit;
    let next_cursor = if more {
        let id = uuid::Uuid::new_v4().to_string();
        let mut state = registry.lock()?;
        if state.discovery_cursors.len() >= 4096 {
            return Err(RegistryBrowserError::Busy);
        }
        state.discovery_cursors.insert(
            id.clone(),
            super::registry::DiscoveryCursor {
                case_id: ctx.case_id.0.clone(),
                source_id: source.0,
                revision: revision.clone(),
                after: entries
                    .get(page_count.saturating_sub(1))
                    .map(|entry| entry.id.0.clone())
                    .ok_or(RegistryBrowserError::Internal)?,
                created: std::time::Instant::now(),
            },
        );
        Some(id)
    } else {
        None
    };
    Ok(RegistryHivePageDto {
        source_revision: revision,
        coverage: mapping::coverage(total_count, hives.len(), 0, false),
        hives,
        diagnostics: Vec::new(),
        next_cursor,
    })
}

fn hive_entries(
    conn: &rusqlite::Connection,
    repo: &FileRepo<'_>,
    source: &DataSourceId,
) -> Result<Vec<domain::FileEntry>, RegistryBrowserError> {
    let artifacts = ArtifactRepo::new(conn)
        .list_by_family(Some("RegistryHive"))
        .map_err(|_| RegistryBrowserError::SourceUnavailable)?;
    let mut entries = Vec::new();
    for artifact in artifacts {
        let Some(id) = artifact.source_object_id else {
            continue;
        };
        let Some(entry) = repo
            .find_by_id(&id)
            .map_err(|_| RegistryBrowserError::SourceUnavailable)?
        else {
            continue;
        };
        if entry.data_source_id == *source && !entry.deleted && !entry.is_directory() {
            entries.push(entry);
        }
    }
    entries.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.id.0.cmp(&right.id.0))
    });
    entries.dedup_by(|left, right| left.id == right.id);
    Ok(entries)
}
