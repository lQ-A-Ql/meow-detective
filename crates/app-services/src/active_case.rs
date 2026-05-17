use chrono::{DateTime, Utc};
use domain::CaseMeta;
use persistence_sqlite::DbResult;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct ActiveCase {
    pub meta: CaseMeta,
    pub case_root: PathBuf,
    pub opened_at: DateTime<Utc>,
    conn: Mutex<Connection>,
}

impl ActiveCase {
    pub fn new(meta: CaseMeta, case_root: PathBuf, conn: Connection) -> Self {
        Self {
            meta,
            case_root,
            opened_at: Utc::now(),
            conn: Mutex::new(conn),
        }
    }

    pub fn with_conn<F, T>(&self, f: F) -> DbResult<T>
    where
        F: FnOnce(&Connection) -> DbResult<T>,
    {
        let conn = self.conn.lock().map_err(|e| {
            persistence_sqlite::DbError::Migration(format!("Lock poisoned: {}", e))
        })?;
        f(&conn)
    }

    pub fn db_path(&self) -> PathBuf {
        self.case_root.join("app.db")
    }
}

unsafe impl Send for ActiveCase {}
unsafe impl Sync for ActiveCase {}
