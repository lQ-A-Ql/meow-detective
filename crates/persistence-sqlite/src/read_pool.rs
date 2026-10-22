use crate::{connection::DbError, DbResult};
use r2d2::{ManageConnection, Pool, PooledConnection};
use rusqlite::{Connection, OpenFlags};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::time::Duration;

const READ_POOL_MAX_SIZE: u32 = 4;

#[derive(Debug, Clone)]
struct ReadOnlyConnectionManager {
    path: PathBuf,
}

impl ManageConnection for ReadOnlyConnectionManager {
    type Connection = Connection;
    type Error = rusqlite::Error;

    fn connect(&self) -> Result<Self::Connection, Self::Error> {
        let connection = Connection::open_with_flags(&self.path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        connection.execute_batch(
            "PRAGMA query_only=ON;
             PRAGMA foreign_keys=ON;
             PRAGMA busy_timeout=30000;",
        )?;
        Ok(connection)
    }

    fn is_valid(&self, connection: &mut Self::Connection) -> Result<(), Self::Error> {
        connection.query_row("SELECT 1", [], |_| Ok(()))
    }

    fn has_broken(&self, _connection: &mut Self::Connection) -> bool {
        false
    }
}

/// A bounded pool of read-only SQLite connections for case/source browsing.
/// It cannot be used for writes because every connection is opened with
/// `SQLITE_OPEN_READ_ONLY` and `PRAGMA query_only=ON`.
pub struct ReadOnlyDbPool {
    pool: Pool<ReadOnlyConnectionManager>,
}

pub struct ReadOnlyDbConnection(PooledConnection<ReadOnlyConnectionManager>);

impl Deref for ReadOnlyDbConnection {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ReadOnlyDbPool {
    pub fn open(path: &Path) -> DbResult<Self> {
        let manager = ReadOnlyConnectionManager {
            path: path.to_path_buf(),
        };
        let pool = Pool::builder()
            .max_size(READ_POOL_MAX_SIZE)
            .min_idle(Some(1))
            .connection_timeout(Duration::from_secs(30))
            .build(manager)
            .map_err(|error| {
                DbError::System(format!("Failed to create read connection pool: {error}"))
            })?;
        Ok(Self { pool })
    }

    pub fn get(&self) -> DbResult<ReadOnlyDbConnection> {
        self.pool
            .get()
            .map(ReadOnlyDbConnection)
            .map_err(|error| DbError::System(format!("Failed to acquire read connection: {error}")))
    }
}
