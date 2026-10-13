use super::{
    mapping,
    registry::{MAX_HIVE_BYTES, TTL},
    session::Session,
    RegistryBrowserContext, RegistryBrowserError, RegistryBrowserRegistry,
};
use artifacts_windows::registry::browser::HiveSnapshot;
use persistence_sqlite::repositories::file_repo::FileRepo;
use std::{collections::HashMap, sync::Arc, time::Instant};
use transport::dto::*;

pub fn open_hive(
    registry: &RegistryBrowserRegistry,
    ctx: &RegistryBrowserContext<'_>,
    request: RegistryHiveOpenRequestDto,
) -> Result<RegistryHiveSessionDto, RegistryBrowserError> {
    if request.file_id.len() > 1024 {
        return Err(RegistryBrowserError::InvalidRequest);
    }
    let (global, conn) = crate::file_service::open_source_for_file_id(
        ctx.case_conn,
        ctx.case_root,
        ctx.case_id,
        &request.file_id,
    )
    .map_err(|_| RegistryBrowserError::SourceUnavailable)?;
    ctx.source(&global.data_source_id)?;
    let revision = ctx.revision(&global.data_source_id)?;
    let entry = ctx.file(&conn, &global.local_id)?;
    let mut descriptor = mapping::descriptor(&entry);
    if descriptor.file_id != request.file_id
        || entry.deleted
        || entry.entry_type != domain::EntryType::File
    {
        return Err(RegistryBrowserError::InvalidRequest);
    }
    if entry.encrypted {
        return Err(RegistryBrowserError::LockedHive);
    }
    let mut state = registry.lock()?;
    let mut context = crate::file_service::SourceReadContext::new(
        &conn,
        ctx.case_conn,
        ctx.case_root,
        ctx.case_id,
        &global.data_source_id,
    )
    .with_bitlocker_runtime(ctx.bitlocker.clone());
    let handle = crate::file_service::open_file_handle_real(&mut context, &global.local_id.0)
        .map_err(|_| RegistryBrowserError::SourceUnavailable)?;
    if handle.size > MAX_HIVE_BYTES as u64 {
        return Err(artifacts_windows::registry::browser::BrowserError::Limit.into());
    }
    // Reserve the full source and bitmap before reading any payload.
    state.reserve(
        handle.size as usize
            + (handle.size as usize).div_ceil(64)
            + super::registry::METADATA_BUDGET,
    )?;
    let bytes = read_snapshot(&mut context, &global.local_id, handle.size as usize)?;
    if bytes.len() as u64 != handle.size {
        return Err(RegistryBrowserError::SourceUnavailable);
    }
    state.reads += 1;
    let hive = HiveSnapshot::open(Arc::new(bytes))?;
    if ctx.revision(&global.data_source_id)? != revision {
        return Err(RegistryBrowserError::ExpiredSession);
    }
    let diagnostics = hive_diagnostics(&hive, has_logs(&conn, &entry));
    let root = hive.root_key()?;
    descriptor.read_state = RegistryHiveReadStateDto::Readable;
    descriptor.byte_length = handle.size;
    let source = RegistryBrowserSourceDto {
        case_id: ctx.case_id.0.clone(),
        data_source_id: global.data_source_id.0,
        file_id: request.file_id,
        source_revision: revision,
        view: request.view,
    };
    register_session(&mut state, hive, source, descriptor, root, diagnostics)
}
fn read_snapshot(
    context: &mut crate::file_service::SourceReadContext<'_>,
    id: &domain::FileEntryId,
    size: usize,
) -> Result<Vec<u8>, RegistryBrowserError> {
    use std::io::Read;
    let source = context
        .open_file_range_by_id(id)
        .map_err(|_| RegistryBrowserError::SourceUnavailable)?;
    let mut reader: Box<dyn Read> = match source {
        crate::file_service::RangeContentReader::Seekable(reader) => reader,
        crate::file_service::RangeContentReader::Streaming(reader) => reader,
    };
    let mut bytes = vec![0; size];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| RegistryBrowserError::SourceUnavailable)?;
    Ok(bytes)
}
fn hive_diagnostics(hive: &HiveSnapshot, logs: bool) -> Vec<RegistryBrowserDiagnosticDto> {
    let mut diagnostics = vec![mapping::diagnostic("baseHiveOnly", None)];
    if hive.is_dirty() {
        diagnostics.push(mapping::diagnostic("dirtyHive", None));
    }
    if hive.is_partial() {
        diagnostics.push(mapping::diagnostic("partialHive", None));
    }
    if logs {
        diagnostics.push(mapping::diagnostic("transactionLogsAvailable", None));
    }
    diagnostics
}
fn register_session(
    state: &mut super::registry::RegistryState,
    hive: HiveSnapshot,
    source: RegistryBrowserSourceDto,
    descriptor: RegistryHiveDescriptorDto,
    root: artifacts_windows::registry::browser::BrowserKey,
    diagnostics: Vec<RegistryBrowserDiagnosticDto>,
) -> Result<RegistryHiveSessionDto, RegistryBrowserError> {
    let mut session = Session {
        source: source.clone(),
        hive,
        last_used: Instant::now(),
        keys: HashMap::new(),
        values: HashMap::new(),
        cursors: HashMap::new(),
        metadata_bytes: 0,
    };
    let root_id = session.issue_key(root.clone())?;
    let id = uuid::Uuid::new_v4().to_string();
    state.sessions.insert(id.clone(), session);
    Ok(RegistryHiveSessionDto {
        session_id: id,
        source,
        hive: descriptor,
        root_key: mapping::key(&root, root_id),
        expires_at: (chrono::Utc::now() + chrono::Duration::seconds(TTL.as_secs() as i64))
            .to_rfc3339(),
        diagnostics,
    })
}
fn has_logs(conn: &rusqlite::Connection, entry: &domain::FileEntry) -> bool {
    let Some(parent) = &entry.parent_id else {
        return false;
    };
    FileRepo::new(conn)
        .find_children(parent)
        .ok()
        .is_some_and(|siblings| {
            siblings.iter().any(|s| {
                !s.deleted
                    && (s.name.eq_ignore_ascii_case(&format!("{}.LOG1", entry.name))
                        || s.name.eq_ignore_ascii_case(&format!("{}.LOG2", entry.name)))
            })
        })
}
