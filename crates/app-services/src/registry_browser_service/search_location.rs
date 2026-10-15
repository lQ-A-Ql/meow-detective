use super::{
    mapping, session::with_session, RegistryBrowserContext, RegistryBrowserError,
    RegistryBrowserRegistry,
};
use transport::dto::{RegistrySearchLocateRequestDto, RegistrySearchLocationDto};
pub fn locate_search_hit(
    registry: &RegistryBrowserRegistry,
    ctx: &RegistryBrowserContext<'_>,
    request: RegistrySearchLocateRequestDto,
) -> Result<RegistrySearchLocationDto, RegistryBrowserError> {
    with_session(registry, ctx, &request.session_id, |session| {
        let key = session.hive.resolve(&request.key_path)?;
        if key.offset != request.key_cell_offset {
            return Err(RegistryBrowserError::InvalidRequest);
        }
        let id = session.issue_key(key.clone())?;
        let mut value = None;
        let mut cursor = None;
        match (request.value_cell_offset, request.value_index) {
            (Some(offset), Some(index)) => {
                let page = session.hive.values(&key, index as usize, 1)?;
                let (_, item) = page
                    .items
                    .into_iter()
                    .next()
                    .ok_or(RegistryBrowserError::InvalidRequest)?;
                let item = item?;
                if item.offset != offset {
                    return Err(RegistryBrowserError::InvalidRequest);
                }
                let value_id = session.issue_value(key.offset, offset)?;
                value = Some(mapping::value(item, value_id));
                let start = index as usize;
                if start > 0 {
                    cursor = session.cursor(Some(start), format!("values:{id}"))?;
                }
            }
            (None, None) => {}
            _ => return Err(RegistryBrowserError::InvalidRequest),
        }
        Ok(RegistrySearchLocationDto {
            key: mapping::key(&key, id),
            value,
            values_cursor: cursor,
        })
    })
}
