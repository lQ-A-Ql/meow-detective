use super::SourceStore;
use crate::{
    database::{DatabaseState, SingleWriterTxDatabase},
    DatabaseConfig, FjallStorageError, Result,
};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, Weak},
};

#[derive(Default)]
struct SharedStores {
    stores: HashMap<PathBuf, Weak<DatabaseState>>,
    closing: HashSet<PathBuf>,
}

static SHARED_STORES: OnceLock<Mutex<SharedStores>> = OnceLock::new();

pub struct SharedSourceStoreCloseGuard {
    prefix: PathBuf,
}

impl Drop for SharedSourceStoreCloseGuard {
    fn drop(&mut self) {
        if let Some(registry) = SHARED_STORES.get() {
            if let Ok(mut registry) = registry.lock() {
                registry.closing.remove(&self.prefix);
            }
        }
    }
}

/// Blocks shared opens under the source directory when no shared database or snapshot is live.
/// The source directory need not exist; admission never creates or reads a database.
pub fn close_shared_source_stores_under(
    path: &Path,
) -> Result<Option<SharedSourceStoreCloseGuard>> {
    let prefix = canonicalize_or_resolve(path)?;
    let mut registry = SHARED_STORES
        .get_or_init(|| Mutex::new(SharedStores::default()))
        .lock()
        .map_err(|_| FjallStorageError::LockPoisoned)?;
    if registry
        .closing
        .iter()
        .any(|closed| is_within(&prefix, closed) || is_within(closed, &prefix))
        || registry
            .stores
            .iter()
            .any(|(path, store)| is_within(path, &prefix) && store.strong_count() > 0)
    {
        return Ok(None);
    }
    registry.stores.retain(|_, store| store.strong_count() > 0);
    registry.closing.insert(prefix.clone());
    Ok(Some(SharedSourceStoreCloseGuard { prefix }))
}

/// Shares the database owner and serializes opening against source retirement.
pub fn open_shared_source_store(
    path: impl AsRef<Path>,
    config: DatabaseConfig,
) -> Result<Arc<SourceStore>> {
    let resolved = canonicalize_or_resolve(path.as_ref())?;
    let mut registry = SHARED_STORES
        .get_or_init(|| Mutex::new(SharedStores::default()))
        .lock()
        .map_err(|_| FjallStorageError::LockPoisoned)?;
    reject_closed(&registry, &resolved)?;
    registry.stores.retain(|_, store| store.strong_count() > 0);
    if let Some(store) = registry.stores.get(&resolved).and_then(Weak::upgrade) {
        return Ok(Arc::new(SourceStore {
            database: SingleWriterTxDatabase { inner: store },
        }));
    }
    std::fs::create_dir_all(&resolved)?;
    let path = std::fs::canonicalize(resolved)?;
    reject_closed(&registry, &path)?;
    registry.stores.retain(|_, store| store.strong_count() > 0);
    if let Some(store) = registry.stores.get(&path).and_then(Weak::upgrade) {
        return Ok(Arc::new(SourceStore {
            database: SingleWriterTxDatabase { inner: store },
        }));
    }
    let store = Arc::new(SourceStore::open(&path, config)?);
    registry
        .stores
        .insert(path, Arc::downgrade(&store.database.inner));
    Ok(store)
}

fn reject_closed(registry: &SharedStores, path: &Path) -> Result<()> {
    if registry
        .closing
        .iter()
        .any(|prefix| is_within(path, prefix))
    {
        return Err(FjallStorageError::StorageLocked {
            path: path.to_owned(),
        });
    }
    Ok(())
}

fn is_within(path: &Path, prefix: &Path) -> bool {
    #[cfg(windows)]
    {
        let mut actual = path.components();
        for expected in prefix.components() {
            let Some(component) = actual.next() else {
                return false;
            };
            if !component
                .as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(&expected.as_os_str().to_string_lossy())
            {
                return false;
            }
        }
        true
    }
    #[cfg(not(windows))]
    {
        path.starts_with(prefix)
    }
}

fn canonicalize_or_resolve(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        return canonicalize_or_resolve(&std::env::current_dir()?.join(path));
    }
    match std::fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path
                .parent()
                .ok_or_else(|| FjallStorageError::InvalidBuildPath(path.display().to_string()))?;
            let name = path
                .file_name()
                .ok_or_else(|| FjallStorageError::InvalidBuildPath(path.display().to_string()))?;
            Ok(canonicalize_or_resolve(parent)?.join(name))
        }
        Err(error) => Err(error.into()),
    }
}
