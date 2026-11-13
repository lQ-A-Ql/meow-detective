//! Source SQL connection lifetimes and exclusive generation reclamation.

use super::generation::bound_source_generation_root;
use domain::DataSourceId;
use persistence_sqlite::{DbError, DbResult};
use rusqlite::{functions::FunctionFlags, Connection};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

const OPEN_WAIT_LIMIT: Duration = Duration::from_secs(2);

#[derive(Default)]
struct Connections {
    readers: HashMap<PathBuf, usize>,
    retiring: HashSet<PathBuf>,
}

#[derive(Default)]
struct ConnectionRegistry {
    state: Mutex<Connections>,
    changed: Condvar,
}

static CONNECTIONS: OnceLock<ConnectionRegistry> = OnceLock::new();

#[derive(Debug, thiserror::Error)]
#[error("source generation reclamation remained busy for {waited:?}")]
pub struct SourceGenerationBusy {
    pub waited: Duration,
}

struct ConnectionPin {
    source_directory: PathBuf,
}

impl ConnectionPin {
    fn acquire(path: &Path) -> DbResult<Self> {
        let parent = path
            .parent()
            .ok_or_else(|| DbError::System("source database has no directory".into()))?;
        let source_directory = std::fs::canonicalize(parent)?;
        let registry = CONNECTIONS.get_or_init(ConnectionRegistry::default);
        let mut connections = registry
            .state
            .lock()
            .map_err(|_| DbError::System("source connection registry is poisoned".into()))?;
        let started = Instant::now();
        let deadline = started + OPEN_WAIT_LIMIT;
        while connections.retiring.contains(&source_directory) {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Err(DbError::Io(std::io::Error::new(
                    std::io::ErrorKind::WouldBlock,
                    SourceGenerationBusy {
                        waited: started.elapsed(),
                    },
                )));
            };
            connections = registry
                .changed
                .wait_timeout(connections, remaining)
                .map_err(|_| DbError::System("source connection registry is poisoned".into()))?
                .0;
        }
        *connections
            .readers
            .entry(source_directory.clone())
            .or_default() += 1;
        Ok(Self { source_directory })
    }

    fn keep_alive(&self) -> i64 {
        0
    }
}

impl Drop for ConnectionPin {
    fn drop(&mut self) {
        let Some(registry) = CONNECTIONS.get() else {
            tracing::error!("source connection registry is missing during close");
            return;
        };
        let Ok(mut connections) = registry.state.lock() else {
            tracing::error!("source connection registry is poisoned during close");
            return;
        };
        let Some(readers) = connections.readers.get_mut(&self.source_directory) else {
            tracing::error!("source connection pin is missing during close");
            return;
        };
        *readers -= 1;
        let became_idle = *readers == 0;
        if became_idle {
            connections.readers.remove(&self.source_directory);
        }
        drop(connections);
        if became_idle {
            crate::publication::notify_generation_source_idle(&self.source_directory);
        }
    }
}

pub(crate) fn open_tracked(
    path: &Path,
    create_parent: bool,
    open: impl FnOnce(&Path) -> DbResult<Connection>,
) -> DbResult<Connection> {
    if create_parent {
        std::fs::create_dir_all(
            path.parent()
                .ok_or_else(|| DbError::System("source database has no directory".into()))?,
        )?;
    }
    let pin = ConnectionPin::acquire(path)?;
    let connection = open(path)?;
    connection.create_scalar_function(
        "__meow_source_connection_lifetime",
        0,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DIRECTONLY,
        move |_| Ok(pin.keep_alive()),
    )?;
    Ok(connection)
}

pub struct SourceGenerationGcGuard {
    source_directory: PathBuf,
}

pub fn try_begin_generation_gc(
    source_directory: &Path,
) -> DbResult<Option<SourceGenerationGcGuard>> {
    let source_directory = std::fs::canonicalize(source_directory)?;
    let mut connections = CONNECTIONS
        .get_or_init(ConnectionRegistry::default)
        .state
        .lock()
        .map_err(|_| DbError::System("source connection registry is poisoned".into()))?;
    if connections.readers.contains_key(&source_directory)
        || connections.retiring.contains(&source_directory)
    {
        return Ok(None);
    }
    connections.retiring.insert(source_directory.clone());
    Ok(Some(SourceGenerationGcGuard { source_directory }))
}

impl SourceGenerationGcGuard {
    pub fn current_generation_root(&self, source_id: &DataSourceId) -> DbResult<PathBuf> {
        let connection = persistence_sqlite::open_existing_source_read_only(
            &self.source_directory.join("source.db"),
        )?;
        let path = bound_source_generation_root(&connection, source_id)?;
        let actual = std::fs::canonicalize(&path)?;
        if actual.parent() != Some(self.source_directory.as_path()) {
            return Err(DbError::System(
                "source generation is outside its source directory".into(),
            ));
        }
        persistence_fjall::read_source_generation_manifest_at_path(&actual, &source_id.0)
            .map_err(|error| DbError::System(error.to_string()))?;
        Ok(actual)
    }

    pub fn try_close_retired_generation(
        &self,
        path: &Path,
    ) -> DbResult<Option<crate::runtime::SourceStoreCloseGuard>> {
        let connection = persistence_sqlite::open_existing_source_read_only(
            &self.source_directory.join("source.db"),
        )?;
        let source = crate::detail::source_id(&connection)?
            .ok_or_else(|| DbError::System("source identity is missing".into()))?;
        let source_id = DataSourceId(source);
        let current = self.current_generation_root(&source_id)?;
        let target = std::fs::canonicalize(path)?;
        if target.parent() != Some(self.source_directory.as_path()) || target == current {
            return Err(DbError::System(
                "generation is not a retired source generation".into(),
            ));
        }
        persistence_fjall::read_source_generation_manifest_at_path(&target, &source_id.0)
            .map_err(|error| DbError::System(error.to_string()))?;
        // Register the shared store so a handle obtained directly through the
        // detail API also participates in the exclusive-ownership check.
        drop(
            crate::runtime::ReadOnlyStorageRuntime::open(&target)
                .map_err(|error| DbError::System(error.to_string()))?,
        );
        crate::runtime::close_source_store_at_path(&target)
            .map_err(|error| DbError::System(error.to_string()))
    }
}

impl Drop for SourceGenerationGcGuard {
    fn drop(&mut self) {
        let Some(registry) = CONNECTIONS.get() else {
            return;
        };
        match registry.state.lock() {
            Ok(mut connections) => {
                connections.retiring.remove(&self.source_directory);
            }
            Err(_) => tracing::error!("source connection registry is poisoned during GC close"),
        }
        registry.changed.notify_all();
    }
}
