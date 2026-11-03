use super::{DatabaseConfig, SourceStore, StorageRuntimeError};
use persistence_fjall::FjallStorageError;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, Weak},
};

const MAX_CACHED_SOURCE_STORES: usize = 4;

#[derive(Default)]
struct Registry {
    stores: HashMap<PathBuf, CachedStore>,
    opening: HashMap<PathBuf, Weak<Mutex<()>>>,
    closing: HashSet<PathBuf>,
    clock: u64,
}

struct CachedStore {
    store: Arc<SourceStore>,
    last_used: u64,
}

static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();

pub(crate) struct SourceStoreCloseGuard {
    blockers: Vec<PathBuf>,
}

impl Drop for SourceStoreCloseGuard {
    fn drop(&mut self) {
        let Some(registry) = REGISTRY.get() else {
            return;
        };
        if let Ok(mut registry) = registry.lock() {
            for path in &self.blockers {
                registry.closing.remove(path);
            }
        }
    }
}

pub(super) fn source_storage_root(
    case_root: &Path,
    source_id: &str,
) -> Result<PathBuf, StorageRuntimeError> {
    validate_source_id(source_id)?;
    Ok(case_root
        .join("sources")
        .join(source_id)
        .join("source.fjall"))
}

fn validate_source_id(source_id: &str) -> Result<(), StorageRuntimeError> {
    let mut components = Path::new(source_id).components();
    let valid = !source_id.is_empty()
        && !source_id.contains('\0')
        && !source_id.contains('/')
        && !source_id.contains('\\')
        && !source_id.contains(':')
        && matches!(components.next(), Some(std::path::Component::Normal(_)))
        && components.next().is_none();
    if valid {
        Ok(())
    } else {
        Err(StorageRuntimeError::InvalidSourceId(source_id.to_owned()))
    }
}

pub(super) fn open_shared_source_store(
    source_root: &Path,
    config: DatabaseConfig,
) -> Result<Arc<SourceStore>, StorageRuntimeError> {
    let source_root = std::fs::canonicalize(source_root)
        .map_err(|error| StorageRuntimeError::Source(FjallStorageError::Io(error)))?;
    let registry = REGISTRY.get_or_init(|| Mutex::new(Registry::default()));
    let open_lock = {
        let mut registry = registry
            .lock()
            .map_err(|_| StorageRuntimeError::RegistryPoisoned)?;
        if registry
            .closing
            .iter()
            .any(|prefix| source_root.starts_with(prefix))
        {
            return Err(StorageRuntimeError::SourceClosing(source_root));
        }
        registry.opening.retain(|_, lock| lock.strong_count() > 0);
        registry.clock = registry.clock.saturating_add(1);
        let last_used = registry.clock;
        if let Some(cached) = registry.stores.get_mut(&source_root) {
            cached.last_used = last_used;
            let store = Arc::clone(&cached.store);
            evict_idle_stores(&mut registry);
            return Ok(store);
        }
        if let Some(lock) = registry.opening.get(&source_root).and_then(Weak::upgrade) {
            lock
        } else {
            let lock = Arc::new(Mutex::new(()));
            registry
                .opening
                .insert(source_root.clone(), Arc::downgrade(&lock));
            lock
        }
    };
    let _open_guard = open_lock
        .lock()
        .map_err(|_| StorageRuntimeError::RegistryPoisoned)?;
    {
        let mut registry = registry
            .lock()
            .map_err(|_| StorageRuntimeError::RegistryPoisoned)?;
        if registry
            .closing
            .iter()
            .any(|prefix| source_root.starts_with(prefix))
        {
            return Err(StorageRuntimeError::SourceClosing(source_root));
        }
        registry.clock = registry.clock.saturating_add(1);
        let last_used = registry.clock;
        if let Some(cached) = registry.stores.get_mut(&source_root) {
            cached.last_used = last_used;
            let store = Arc::clone(&cached.store);
            evict_idle_stores(&mut registry);
            return Ok(store);
        }
    }
    let store = Arc::new(SourceStore::open(&source_root, config)?);
    let mut registry = registry
        .lock()
        .map_err(|_| StorageRuntimeError::RegistryPoisoned)?;
    registry.clock = registry.clock.saturating_add(1);
    let last_used = registry.clock;
    registry.stores.insert(
        source_root,
        CachedStore {
            store: Arc::clone(&store),
            last_used,
        },
    );
    evict_idle_stores(&mut registry);
    Ok(store)
}

pub(crate) fn close_source_store(
    case_root: impl AsRef<Path>,
    source_id: &str,
) -> Result<Option<SourceStoreCloseGuard>, StorageRuntimeError> {
    let source_root = source_storage_root(case_root.as_ref(), source_id)?;
    close_source_store_at_path(&source_root)
}

pub(crate) fn close_source_store_at_path(
    source_root: &Path,
) -> Result<Option<SourceStoreCloseGuard>, StorageRuntimeError> {
    let source_root = canonicalize_or_resolve(source_root)?;
    let registry = REGISTRY.get_or_init(|| Mutex::new(Registry::default()));
    let cached = {
        let mut registry = registry
            .lock()
            .map_err(|_| StorageRuntimeError::RegistryPoisoned)?;
        if registry
            .opening
            .get(&source_root)
            .and_then(Weak::upgrade)
            .is_some()
            || registry
                .stores
                .get(&source_root)
                .is_some_and(|cached| Arc::strong_count(&cached.store) > 1)
        {
            return Ok(None);
        }
        registry.closing.insert(source_root.clone());
        registry.stores.remove(&source_root)
    };
    drop(cached);
    Ok(Some(SourceStoreCloseGuard {
        blockers: vec![source_root],
    }))
}

pub(crate) fn close_case_source_stores(
    case_root: &Path,
) -> Result<Option<SourceStoreCloseGuard>, StorageRuntimeError> {
    let case_root = canonicalize_or_resolve(case_root)?;
    let sources_root = case_root.join("sources");
    let registry = REGISTRY.get_or_init(|| Mutex::new(Registry::default()));
    let removed = {
        let mut registry = registry
            .lock()
            .map_err(|_| StorageRuntimeError::RegistryPoisoned)?;
        let paths = registry
            .stores
            .keys()
            .filter(|path| path.starts_with(&sources_root))
            .cloned()
            .collect::<Vec<_>>();
        if registry
            .opening
            .iter()
            .any(|(path, lock)| path.starts_with(&sources_root) && lock.strong_count() > 0)
            || paths.iter().any(|path| {
                registry
                    .stores
                    .get(path)
                    .is_some_and(|cached| Arc::strong_count(&cached.store) > 1)
            })
        {
            return Ok(None);
        }
        registry.closing.insert(sources_root.clone());
        let removed = paths
            .iter()
            .filter_map(|path| registry.stores.remove(path))
            .collect::<Vec<_>>();
        removed
    };
    drop(removed);
    Ok(Some(SourceStoreCloseGuard {
        blockers: vec![sources_root],
    }))
}

fn canonicalize_or_resolve(path: &Path) -> Result<PathBuf, StorageRuntimeError> {
    match std::fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let missing = || {
                StorageRuntimeError::Source(FjallStorageError::Io(std::io::Error::new(
                    error.kind(),
                    error.to_string(),
                )))
            };
            let parent = path.parent().ok_or_else(missing)?;
            let file_name = path.file_name().ok_or_else(missing)?;
            Ok(canonicalize_or_resolve(parent)?.join(file_name))
        }
        Err(error) => Err(StorageRuntimeError::Source(FjallStorageError::Io(error))),
    }
}

fn evict_idle_stores(registry: &mut Registry) {
    while registry.stores.len() > MAX_CACHED_SOURCE_STORES {
        let victim = registry
            .stores
            .iter()
            .filter(|(_, cached)| Arc::strong_count(&cached.store) == 1)
            .min_by_key(|(_, cached)| cached.last_used)
            .map(|(path, _)| path.clone());
        let Some(victim) = victim else {
            return;
        };
        registry.stores.remove(&victim);
    }
}
