use super::{mapping, RegistryBrowserContext, RegistryBrowserError, RegistryBrowserRegistry};
use domain::DataSourceId;
use persistence_sqlite::repositories::file_repo::FileRepo;
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
    let mut state = registry.lock()?;
    let after = match request.cursor {
        None => String::new(),
        Some(id) => {
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
        }
    };
    let repo = FileRepo::new(&conn);
    let count = repo
        .registry_hive_candidate_count(&source)
        .map_err(|_| RegistryBrowserError::SourceUnavailable)?;
    let mut entries = repo
        .registry_hive_candidates(&source, &after, limit as u32 + 1)
        .map_err(|_| RegistryBrowserError::SourceUnavailable)?;
    let more = entries.len() > limit;
    entries.truncate(limit);
    let next_cursor = if more {
        if state.discovery_cursors.len() >= 4096 {
            return Err(RegistryBrowserError::Busy);
        }
        let id = uuid::Uuid::new_v4().to_string();
        state.discovery_cursors.insert(
            id.clone(),
            super::registry::DiscoveryCursor {
                case_id: ctx.case_id.0.clone(),
                source_id: source.0,
                revision: revision.clone(),
                after: entries
                    .last()
                    .ok_or(RegistryBrowserError::Internal)?
                    .id
                    .0
                    .clone(),
                created: std::time::Instant::now(),
            },
        );
        Some(id)
    } else {
        None
    };
    let hives = entries.iter().map(mapping::descriptor).collect::<Vec<_>>();
    Ok(RegistryHivePageDto {
        source_revision: revision,
        coverage: mapping::coverage(count, hives.len(), 0, false),
        hives,
        diagnostics: Vec::new(),
        next_cursor,
    })
}
