use crate::{BrowserError, Result};
use rusqlite::{serialize::OwnedData, Connection, DatabaseName, OpenFlags};
use std::{
    io::Write,
    path::Path,
    ptr::NonNull,
    time::{Duration, Instant},
};

pub const MAX_DATABASE_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_WAL_BYTES: usize = 64 * 1024 * 1024;

/// The SQLite-deserialization buffer and its connection are owned by SQLite.
/// WAL journal markers are normalized only in this derived, DB-only image,
/// as required by sqlite3_deserialize; the caller's bytes remain unchanged.
pub fn open_sqlite_from_bytes(bytes: &[u8]) -> Result<Connection> {
    if bytes.len() < 100 {
        return Err(BrowserError::Parse("database header is truncated".into()));
    }
    if bytes.len() > MAX_DATABASE_BYTES {
        return Err(BrowserError::InputLimit);
    }
    let capacity = bytes
        .len()
        .checked_add(32)
        .ok_or(BrowserError::InputLimit)?;
    // SAFETY: The requested bounded allocation is used only through its uniquely owned pointer.
    let ptr =
        NonNull::new(unsafe { rusqlite::ffi::sqlite3_malloc64(capacity as u64) }.cast::<u8>())
            .ok_or(BrowserError::Allocation)?;
    // SAFETY: SQLite allocated capacity writable bytes; the input does not alias that new allocation.
    let target = unsafe { std::slice::from_raw_parts_mut(ptr.as_ptr(), capacity) };
    target.fill(0);
    target[..bytes.len()].copy_from_slice(bytes);
    if bytes.starts_with(b"SQLite format 3\0") && bytes[18] == 2 && bytes[19] == 2 {
        target[18] = 1;
        target[19] = 1;
    }
    // SAFETY: ptr was allocated by sqlite3_malloc64 and ownership transfers to OwnedData.
    let data = unsafe { OwnedData::from_raw_nonnull(ptr, bytes.len()) };
    let mut connection = Connection::open_in_memory()?;
    connection.deserialize(DatabaseName::Main, data, true)?;
    configure(&connection)?;
    Ok(connection)
}

pub struct DatabaseSnapshot {
    connection: Connection,
    _files: tempfile::TempDir,
    wal_included: bool,
}

impl DatabaseSnapshot {
    pub fn prepare(
        parent: &Path,
        bytes: &[u8],
        wal: Option<&[u8]>,
        shm: Option<&[u8]>,
    ) -> Result<Self> {
        if bytes.len() > MAX_DATABASE_BYTES
            || wal.is_some_and(|bytes| bytes.len() > MAX_WAL_BYTES)
            || shm.is_some_and(|bytes| bytes.len() > 8 * 1024 * 1024)
        {
            return Err(BrowserError::InputLimit);
        }
        std::fs::create_dir_all(parent)?;
        let files = tempfile::Builder::new()
            .prefix("browser-")
            .tempdir_in(parent)?;
        write(&files.path().join("snapshot.sqlite"), bytes)?;
        if let Some(bytes) = wal {
            write(&files.path().join("snapshot.sqlite-wal"), bytes)?;
        }
        if let Some(bytes) = shm {
            write(&files.path().join("snapshot.sqlite-shm"), bytes)?;
        }
        let connection = Connection::open_with_flags(
            files.path().join("snapshot.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        configure(&connection)?;
        connection.execute_batch("BEGIN")?;
        connection.query_row("SELECT count(*) FROM sqlite_schema", [], |row| {
            row.get::<_, i64>(0)
        })?;
        Ok(Self {
            connection,
            _files: files,
            wal_included: wal.is_some(),
        })
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }
    pub fn input_mode(&self) -> &'static str {
        if self.wal_included {
            "databaseWithWal"
        } else {
            "databaseOnly"
        }
    }
}

fn configure(connection: &Connection) -> Result<()> {
    connection.busy_timeout(Duration::from_millis(50))?;
    connection.execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF; PRAGMA temp_store=MEMORY; PRAGMA cache_size=-8192;")?;
    refresh_query_budget(connection);
    Ok(())
}

pub(crate) fn refresh_query_budget(connection: &Connection) {
    let deadline = Instant::now() + Duration::from_secs(30);
    connection.progress_handler(1000, Some(move || Instant::now() >= deadline));
}

fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.flush()?;
    Ok(())
}

pub fn table_exists(connection: &Connection, table: &str) -> Result<bool> {
    Ok(connection.query_row(
        "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name=?1",
        [table],
        |row| row.get::<_, i64>(0),
    )? != 0)
}

pub fn table_columns(connection: &Connection, table: &str) -> Result<Vec<String>> {
    let mut statement = connection.prepare("SELECT name FROM pragma_table_info(?1)")?;
    let columns = statement.query_map([table], |row| row.get::<_, String>(0))?;
    columns
        .collect::<std::result::Result<_, _>>()
        .map_err(Into::into)
}
