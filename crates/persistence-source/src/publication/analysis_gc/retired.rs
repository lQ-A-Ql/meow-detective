use super::{
    identity,
    intent::{self, PendingIntent},
};
use crate::control::connections::SourceGenerationGcGuard;
use domain::DataSourceId;
use persistence_sqlite::{DbError, DbResult};
use std::path::Path;

struct ClosedTarget {
    _runtime: crate::runtime::SourceStoreCloseGuard,
    _shared: persistence_fjall::SharedSourceStoreCloseGuard,
}

fn close(path: &Path) -> DbResult<Option<ClosedTarget>> {
    let Some(runtime) = crate::runtime::close_source_store_at_path(path)
        .map_err(|error| DbError::System(error.to_string()))?
    else {
        return Ok(None);
    };
    let Some(shared) = persistence_fjall::close_shared_source_stores_under(path)
        .map_err(|error| DbError::System(error.to_string()))?
    else {
        return Ok(None);
    };
    Ok(Some(ClosedTarget {
        _runtime: runtime,
        _shared: shared,
    }))
}

pub(super) fn stage(
    _guard: &SourceGenerationGcGuard,
    directory: &Path,
    current: &Path,
    original: &Path,
    source: &DataSourceId,
) -> DbResult<Option<PendingIntent>> {
    if original == current || original.parent() != Some(directory) {
        return Err(DbError::System(
            "cleanup target is not a retired generation".into(),
        ));
    }
    let Some(_closed) = close(original)? else {
        return Ok(None);
    };
    persistence_fjall::read_source_generation_manifest_at_path(original, &source.0)
        .map_err(|error| DbError::System(error.to_string()))?;
    let intent = intent::create(directory, original, source)?;
    let quarantine = intent.quarantine(directory);
    let Some(_quarantine) = close(&quarantine)? else {
        return Ok(Some(intent));
    };
    identity::rename(original, &quarantine)?;
    Ok(Some(intent))
}

pub(super) fn start(
    guard: &SourceGenerationGcGuard,
    directory: &Path,
    current: &Path,
    original: &Path,
    source: &DataSourceId,
) -> DbResult<bool> {
    let Some(intent) = stage(guard, directory, current, original, source)? else {
        return Ok(false);
    };
    resume(guard, directory, current, intent)
}

pub(super) fn resume(
    _guard: &SourceGenerationGcGuard,
    directory: &Path,
    current: &Path,
    intent: PendingIntent,
) -> DbResult<bool> {
    let original = intent.original(directory);
    let quarantine = intent.quarantine(directory);
    if quarantine.try_exists()? {
        return remove_quarantine(directory, current, intent);
    }
    if !original.try_exists()? {
        intent::finish(intent)?;
        return Ok(true);
    }
    if original == current || !intent.matches(&original)? {
        return Err(DbError::System(
            "authorized cleanup directory identity changed".into(),
        ));
    }
    let Some(_closed) = close(&original)? else {
        return Ok(false);
    };
    let Some(_quarantine) = close(&quarantine)? else {
        return Ok(false);
    };
    identity::rename(&original, &quarantine)?;
    if !intent.matches(&quarantine)? {
        return Err(DbError::System(
            "quarantined cleanup directory identity changed".into(),
        ));
    }
    std::fs::remove_dir_all(&quarantine)?;
    intent::finish(intent)?;
    Ok(true)
}

fn remove_quarantine(directory: &Path, current: &Path, intent: PendingIntent) -> DbResult<bool> {
    let quarantine = intent.quarantine(directory);
    if quarantine == current || !intent.matches(&quarantine)? {
        return Err(DbError::System(
            "quarantined cleanup directory identity changed".into(),
        ));
    }
    let Some(_closed) = close(&quarantine)? else {
        return Ok(false);
    };
    if !intent.matches(&quarantine)? {
        return Err(DbError::System(
            "quarantined cleanup directory identity changed".into(),
        ));
    }
    std::fs::remove_dir_all(&quarantine)?;
    intent::finish(intent)?;
    Ok(true)
}
