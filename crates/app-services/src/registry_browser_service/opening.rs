use super::{
    io::read_snapshot,
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
        || entry.data_source_id != global.data_source_id
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
    let companions = FileRepo::new(&conn)
        .registry_log_companions(&global.data_source_id, &entry.path)
        .map_err(|_| RegistryBrowserError::SourceUnavailable)?;
    let has_logs = !companions.is_empty();
    let (hive, recovery) = load_view(
        &mut state,
        &mut context,
        &entry,
        handle.size as usize,
        &request.view,
        companions,
    )?;
    if ctx.revision(&global.data_source_id)? != revision {
        return Err(RegistryBrowserError::ExpiredSession);
    }
    let diagnostics = hive_diagnostics(&hive, has_logs);
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
    let mut result = register_session(&mut state, hive, source, descriptor, root, diagnostics)?;
    result.recovery = recovery;
    if result.recovery.is_some() {
        result.diagnostics.retain(|d| {
            d.code != "baseHiveOnly"
                && d.code != "dirtyHive"
                && d.code != "transactionLogsAvailable"
        });
    }
    Ok(result)
}
type HiveViewLoad = (HiveSnapshot, Option<RegistryRecoveryDto>);

fn load_view(
    state: &mut super::registry::RegistryState,
    context: &mut crate::file_service::SourceReadContext<'_>,
    entry: &domain::FileEntry,
    size: usize,
    view: &RegistryHiveViewDto,
    companions: Vec<domain::FileEntry>,
) -> Result<HiveViewLoad, RegistryBrowserError> {
    match view {
        RegistryHiveViewDto::Base => {
            let bytes = read_snapshot(context, &entry.id, size, size)?;
            state.reads += 1;
            Ok((HiveSnapshot::open(Arc::new(bytes))?, None))
        }
        RegistryHiveViewDto::Recovered => {
            let (hive, recovery) =
                super::recovered_view::load(state, context, entry, size, companions)?;
            Ok((hive, Some(recovery)))
        }
    }
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
        recovery: None,
    })
}
