//! Connections retain shared case access until the final SQLite handle drops.
use std::ops::Deref;
use std::sync::{Arc, Mutex};

use tokio::sync::OwnedRwLockReadGuard;
use transport::CommandError;

use super::AppState;

pub(crate) type CaseAccess = Arc<OwnedRwLockReadGuard<()>>;

pub type SharedCaseReadPool = Arc<
    Mutex<
        Option<(
            std::path::PathBuf,
            Arc<persistence_sqlite::read_pool::ReadOnlyDbPool>,
        )>,
    >,
>;

pub struct CaseWriteConnection {
    // Drop the database handle before releasing case access.
    connection: rusqlite::Connection,
    _access: CaseAccess,
}

pub struct CaseReadConnection {
    connection: persistence_sqlite::read_pool::ReadOnlyDbConnection,
    _access: CaseAccess,
}

impl CaseWriteConnection {
    pub(super) fn new(connection: rusqlite::Connection, access: CaseAccess) -> Self {
        Self {
            connection,
            _access: access,
        }
    }
}

impl CaseReadConnection {
    pub(super) fn new(
        connection: persistence_sqlite::read_pool::ReadOnlyDbConnection,
        access: CaseAccess,
    ) -> Self {
        Self {
            connection,
            _access: access,
        }
    }
}

impl Deref for CaseWriteConnection {
    type Target = rusqlite::Connection;

    fn deref(&self) -> &Self::Target {
        &self.connection
    }
}

impl Deref for CaseReadConnection {
    type Target = rusqlite::Connection;

    fn deref(&self) -> &Self::Target {
        &self.connection
    }
}

impl std::fmt::Debug for CaseWriteConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaseWriteConnection")
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for CaseReadConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaseReadConnection").finish_non_exhaustive()
    }
}

impl AppState {
    pub(crate) fn clear_case_read_pool(&self) -> Result<(), CommandError> {
        let mut pool = self
            .read_pool
            .lock()
            .map_err(|error| CommandError::from_lock_error("Case read pool", error))?;
        *pool = None;
        Ok(())
    }

    /// Apply the canonical writer policy during an exclusive lifecycle operation.
    pub fn init_db_pragmas(&self) -> Result<(), String> {
        self.get_transition_connection().map(drop)
    }

    /// Get a fresh writable connection to the active case's database.
    /// Read-only browsing and preview commands use `get_read_connection`.
    pub fn get_connection(&self) -> Result<CaseWriteConnection, String> {
        let access = self.case_access().map_err(|error| error.message)?;
        let connection = self.get_transition_connection()?;
        Ok(CaseWriteConnection::new(connection, access))
    }

    /// Lifecycle code already owns exclusive case access and cannot acquire
    /// another shared lease. This path never creates or migrates a database.
    pub(crate) fn get_transition_connection(&self) -> Result<rusqlite::Connection, String> {
        let guard = self
            .active_case
            .lock()
            .map_err(|e| format!("Lock poisoned: {}", e))?;
        let active = guard
            .as_ref()
            .ok_or("No active case — open or create a case first")?;
        let db_path = active.db_path();
        drop(guard);
        persistence_sqlite::connection::open_existing(&db_path)
            .map_err(|error| format!("Failed to open database: {error}"))
    }

    /// Acquire a pooled read-only case connection. The pool is replaced when
    /// the active case path changes, so a closed case cannot retain its DB.
    pub fn get_read_connection(&self) -> Result<CaseReadConnection, String> {
        let access = self.case_access().map_err(|error| error.message)?;
        let db_path = {
            let guard = self
                .active_case
                .lock()
                .map_err(|e| format!("Lock poisoned: {e}"))?;
            guard
                .as_ref()
                .ok_or("No active case — open or create a case first")?
                .db_path()
        };
        let pool = self.read_pool_for(&db_path)?;
        // Do not hold the registry mutex while waiting for a pool slot. A
        // long-running read must not block case close or replacement of the
        // pool for another active case.
        let connection = pool.get().map_err(|error| error.to_string())?;
        Ok(CaseReadConnection::new(connection, access))
    }

    fn read_pool_for(
        &self,
        db_path: &std::path::Path,
    ) -> Result<Arc<persistence_sqlite::read_pool::ReadOnlyDbPool>, String> {
        {
            let pool = self.read_pool.lock().map_err(|error| error.to_string())?;
            if let Some((_, pool)) = pool.as_ref().filter(|(path, _)| path == db_path) {
                return Ok(Arc::clone(pool));
            }
        }
        // Opening SQLite/r2d2 can block. Keep it outside the registry lock.
        let candidate = Arc::new(
            persistence_sqlite::read_pool::ReadOnlyDbPool::open(db_path)
                .map_err(|error| error.to_string())?,
        );
        let mut pool = self.read_pool.lock().map_err(|error| error.to_string())?;
        if pool.as_ref().is_none_or(|(path, _)| path != db_path) {
            *pool = Some((db_path.to_path_buf(), candidate));
        }
        Ok(Arc::clone(&pool.as_ref().expect("read pool initialized").1))
    }

    /// Clear the database state on case close.
    pub fn clear_db_state(&self) -> Result<(), String> {
        let _access = self
            .case_lifecycle
            .try_write()
            .map_err(|_| "Case operations must drain before clearing database state".to_string())?;
        let mut guard = self
            .active_case
            .lock()
            .map_err(|e| format!("Lock poisoned: {}", e))?;
        *guard = None;
        let mut read_pool = self
            .read_pool
            .lock()
            .map_err(|e| format!("Lock poisoned: {}", e))?;
        *read_pool = None;
        Ok(())
    }

    pub(crate) fn case_access(&self) -> Result<CaseAccess, CommandError> {
        self.case_lifecycle
            .clone()
            .try_read_owned()
            .map(Arc::new)
            .map_err(|_| CommandError::conflict("Case lifecycle transition is in progress"))
    }

    pub(crate) async fn case_transition(
        &self,
    ) -> Result<Arc<tokio::sync::OwnedRwLockWriteGuard<()>>, CommandError> {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.case_lifecycle.clone().write_owned(),
        )
        .await
        .map(Arc::new)
        .map_err(|_| CommandError::timeout("Timed out waiting for active case operations"))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/state/case_database.rs"]
mod tests;
