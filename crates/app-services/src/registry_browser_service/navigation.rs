use super::{
    mapping, session::with_session, RegistryBrowserContext, RegistryBrowserError,
    RegistryBrowserRegistry,
};
use transport::dto::*;

pub fn resolve_key(
    registry: &RegistryBrowserRegistry,
    ctx: &RegistryBrowserContext<'_>,
    request: RegistryKeyResolveRequestDto,
) -> Result<RegistryKeyMetadataDto, RegistryBrowserError> {
    with_session(registry, ctx, &request.session_id, |s| {
        let key = s.hive.resolve(&request.key_path)?;
        let id = s.issue_key(key.clone())?;
        Ok(mapping::key(&key, id))
    })
}
pub fn list_subkeys(
    registry: &RegistryBrowserRegistry,
    ctx: &RegistryBrowserContext<'_>,
    request: RegistrySubkeysRequestDto,
) -> Result<RegistrySubkeyPageDto, RegistryBrowserError> {
    let limit = mapping::page_limit(request.limit)?;
    with_session(registry, ctx, &request.session_id, |s| {
        let key = s.key(&request.key_id)?;
        let scope = format!("keys:{}", request.key_id);
        let page = s
            .hive
            .subkeys(&key, s.start(request.cursor.as_deref(), &scope)?, limit)?;
        let mut diagnostics = mapping::list_diagnostics(page.diagnostic);
        let mut keys = Vec::new();
        for (offset, item) in page.items {
            match item {
                Ok(key) => {
                    let id = s.issue_key(key.clone())?;
                    keys.push(mapping::key(&key, id));
                }
                Err(_) => {
                    let id = format!("corrupt-{offset}");
                    diagnostics.push(mapping::diagnostic("corruptKey", Some(id.clone())));
                    keys.push(mapping::corrupt_key(offset, id));
                }
            }
        }
        Ok(RegistrySubkeyPageDto {
            coverage: mapping::coverage(
                page.declared as u64,
                keys.len(),
                page.skipped,
                !diagnostics.is_empty(),
            ),
            keys,
            diagnostics,
            next_cursor: s.cursor(page.next, scope)?,
        })
    })
}
pub fn list_values(
    registry: &RegistryBrowserRegistry,
    ctx: &RegistryBrowserContext<'_>,
    request: RegistryValuesRequestDto,
) -> Result<RegistryValuePageDto, RegistryBrowserError> {
    let limit = mapping::page_limit(request.limit)?;
    with_session(registry, ctx, &request.session_id, |s| {
        let key = s.key(&request.key_id)?;
        let scope = format!("values:{}", request.key_id);
        let page = s
            .hive
            .values(&key, s.start(request.cursor.as_deref(), &scope)?, limit)?;
        let mut diagnostics = mapping::list_diagnostics(page.diagnostic);
        let mut values = Vec::new();
        for (offset, item) in page.items {
            match item {
                Ok(value) => {
                    let id = s
                        .values
                        .iter()
                        .find(|(_, known)| **known == (key.offset, offset))
                        .map(|(id, _)| id.clone())
                        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                    if s.values.len() >= 16_384 {
                        return Err(RegistryBrowserError::Busy);
                    }
                    if !s.values.contains_key(&id) {
                        s.charge(256)?;
                    }
                    s.values.insert(id.clone(), (key.offset, offset));
                    if value.data_error.is_some() {
                        diagnostics.push(mapping::diagnostic("corruptValue", Some(id.clone())));
                    }
                    values.push(mapping::value(value, id));
                }
                Err(_) => {
                    let id = format!("corrupt-{offset}");
                    diagnostics.push(mapping::diagnostic("corruptValue", Some(id.clone())));
                    values.push(mapping::corrupt_value(offset, id));
                }
            }
        }
        Ok(RegistryValuePageDto {
            coverage: mapping::coverage(
                page.declared as u64,
                values.len(),
                page.skipped,
                !diagnostics.is_empty(),
            ),
            values,
            diagnostics,
            next_cursor: s.cursor(page.next, scope)?,
        })
    })
}
pub fn read_value_range(
    registry: &RegistryBrowserRegistry,
    ctx: &RegistryBrowserContext<'_>,
    request: RegistryValueRangeRequestDto,
) -> Result<RegistryValueRangeDto, RegistryBrowserError> {
    with_session(registry, ctx, &request.session_id, |s| {
        let key = s.key(&request.key_id)?;
        let (parent, offset) = s
            .values
            .get(&request.value_id)
            .copied()
            .ok_or(RegistryBrowserError::InvalidRequest)?;
        if parent != key.offset {
            return Err(RegistryBrowserError::InvalidRequest);
        }
        let value = s.hive.value(offset)?;
        let range = s
            .hive
            .value_range(&value, request.offset, request.length as usize)?;
        let eof = request.offset + range.bytes.len() as u64 >= range.total_length as u64;
        let decode_status = if range.partial {
            RegistryValueDecodeStatusDto::Partial
        } else if range.decoded.is_some() {
            RegistryValueDecodeStatusDto::Decoded
        } else {
            RegistryValueDecodeStatusDto::RawOnly
        };
        Ok(RegistryValueRangeDto {
            value_id: request.value_id,
            offset: request.offset,
            total_length: range.total_length as u64,
            bytes: range.bytes,
            eof,
            decode_status,
            diagnostics: Vec::new(),
            decoded_text: range.decoded,
            overlay_source: match range.overlay_source {
                artifacts_windows::registry::browser::BrowserOverlaySource::Base => {
                    RegistryOverlaySourceDto::Base
                }
                artifacts_windows::registry::browser::BrowserOverlaySource::Recovered => {
                    RegistryOverlaySourceDto::Recovered
                }
                artifacts_windows::registry::browser::BrowserOverlaySource::Merged => {
                    RegistryOverlaySourceDto::Merged
                }
            },
        })
    })
}

pub fn search_hive(
    registry: &RegistryBrowserRegistry,
    ctx: &RegistryBrowserContext<'_>,
    request: RegistrySearchRequestDto,
) -> Result<RegistrySearchPageDto, RegistryBrowserError> {
    search_hive_with_cancel(
        registry,
        ctx,
        request,
        &std::sync::atomic::AtomicBool::new(false),
        &|_, _| {},
    )
}
pub fn search_hive_with_cancel(
    registry: &RegistryBrowserRegistry,
    ctx: &RegistryBrowserContext<'_>,
    request: RegistrySearchRequestDto,
    cancel: &std::sync::atomic::AtomicBool,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<RegistrySearchPageDto, RegistryBrowserError> {
    let snapshot = with_session(registry, ctx, &request.session_id, |session| {
        Ok(session.hive.clone())
    })?;
    // Immutable bytes let navigation and case retirement proceed during search.
    let page = snapshot.search_with_cancel(&request.query, cancel, progress)?;
    Ok(RegistrySearchPageDto {
        hits: page
            .hits
            .into_iter()
            .map(|hit| RegistrySearchHitDto {
                kind: hit.kind.into(),
                path: hit.path,
                name: hit.name,
                cell_offset: hit.cell_offset,
                matched_field: hit.matched_field.into(),
                preview: hit.preview,
                key_cell_offset: hit.key_offset,
                value_index: hit.value_index,
                data_offset: hit.data_offset,
                overlay_source: search_overlay(hit.overlay_source),
                data_overlay_source: search_overlay(hit.data_overlay_source),
            })
            .collect(),
        scanned_keys: page.scanned_keys,
        scanned_values: page.scanned_values,
        skipped_count: page.skipped,
        truncated: page.truncated,
        total_matches: page.total_matches,
        complete: page.complete,
    })
}

fn search_overlay(
    source: artifacts_windows::registry::browser::BrowserOverlaySource,
) -> RegistryOverlaySourceDto {
    match source {
        artifacts_windows::registry::browser::BrowserOverlaySource::Base => {
            RegistryOverlaySourceDto::Base
        }
        artifacts_windows::registry::browser::BrowserOverlaySource::Recovered => {
            RegistryOverlaySourceDto::Recovered
        }
        artifacts_windows::registry::browser::BrowserOverlaySource::Merged => {
            RegistryOverlaySourceDto::Merged
        }
    }
}
