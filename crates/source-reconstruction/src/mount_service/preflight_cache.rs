use super::{emulation::inspect_preflight, MountServiceError};
use domain::{CaseId, DataSourceId};
use moka::sync::Cache;
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use rusqlite::Connection;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        LazyLock,
    },
    time::SystemTime,
};
use transport::dto::EmulationPreflightDto;

#[derive(Clone, Hash, PartialEq, Eq)]
struct Identity {
    case: String,
    source: String,
    root: PathBuf,
    generation: PathBuf,
    manifest: String,
    platform: String,
    kind: String,
    files: Vec<(PathBuf, u64, SystemTime)>,
    epoch: u64,
}

static EPOCH: AtomicU64 = AtomicU64::new(0);
#[derive(Clone)]
pub(super) struct Facts {
    pub preflight: EmulationPreflightDto,
    pub guest: Option<super::LinuxGuestProfile>,
}
static CACHE: LazyLock<Cache<Identity, Facts>> = LazyLock::new(|| Cache::new(32));

pub fn clear_emulation_preflight_cache() {
    EPOCH.fetch_add(1, Ordering::AcqRel);
    CACHE.invalidate_all();
}

fn identity(
    conn: &Connection,
    root: &Path,
    case: &CaseId,
    source: &DataSourceId,
) -> Result<Identity, MountServiceError> {
    source_kernel::source_db::resolve_ready_source_platform(conn, case, source)
        .map_err(|error| MountServiceError::SourceNotReady(error.to_string()))?;
    let prepared = super::prepare_physical_mount_source(conn, source)?;
    let repo = DataSourceRepo::new(conn);
    let kind = repo.source_kind(source)?;
    let files = super::preflight_identity::evidence_files(&prepared.source_path, &kind, &case.0)?;
    let generation = persistence_source::control::current_source_generation_root(root, source)?;
    let manifest = persistence_source::fjall::source_generation_manifest_digest(&generation)
        .map_err(|error| MountServiceError::Catalog(error.to_string()))?;
    Ok(Identity {
        case: case.0.clone(),
        source: source.0.clone(),
        root: root.canonicalize()?,
        generation,
        manifest,
        kind: kind.to_string(),
        platform: repo
            .find_storage(source)?
            .expect("ready source metadata")
            .platform,
        files,
        epoch: EPOCH.load(Ordering::Acquire),
    })
}

fn facts(
    conn: &Connection,
    root: &Path,
    case: &CaseId,
    source: &DataSourceId,
) -> Result<Facts, MountServiceError> {
    let key = identity(conn, root, case, source)?;
    let result = CACHE.try_get_with(key.clone(), || {
        let started = std::time::Instant::now();
        let result = inspect_preflight(conn, root, case, source)?;
        if identity(conn, root, case, source)? != key {
            return Err(MountServiceError::SourceNotReady("source changed during preflight".into()));
        }
        tracing::info!(case_id = %case.0, data_source_id = %source.0, elapsed_ms = started.elapsed().as_millis() as u64, "Emulation preflight computed");
        Ok(result)
    }).map_err(MountServiceError::Preflight)?;
    if identity(conn, root, case, source)? != key {
        CACHE.invalidate(&key);
        return Err(MountServiceError::SourceNotReady(
            "source changed during preflight".into(),
        ));
    }
    Ok(result)
}

pub(super) fn read(
    conn: &Connection,
    root: &Path,
    case: &CaseId,
    source: &DataSourceId,
) -> Result<EmulationPreflightDto, MountServiceError> {
    Ok(facts(conn, root, case, source)?.preflight)
}

pub(super) fn guest_profile(
    conn: &Connection,
    root: &Path,
    case: &CaseId,
    source: &DataSourceId,
) -> Result<super::LinuxGuestProfile, MountServiceError> {
    facts(conn, root, case, source)?
        .guest
        .ok_or_else(|| MountServiceError::Unsupported("source has no Linux guest profile".into()))
}
