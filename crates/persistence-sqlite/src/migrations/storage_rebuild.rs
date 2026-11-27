use crate::{DbError, DbResult};
use rusqlite::Connection;

pub(super) struct StorageRebuild<'a> {
    conn: &'a Connection,
    enabled: bool,
}

impl<'a> StorageRebuild<'a> {
    pub fn prepare(conn: &'a Connection, name: &str) -> DbResult<Self> {
        let enabled = matches!(
            name,
            "0072_storage_object_versions" | "0073_structure_core_s01"
        );
        if enabled {
            if !conn.is_autocommit() {
                return Err(DbError::Migration(
                    "storage rebuild requires an independent migration transaction".into(),
                ));
            }
            conn.execute_batch("PRAGMA foreign_keys=OFF")?;
        }
        Ok(Self { conn, enabled })
    }

    pub fn validate(&self) -> DbResult<()> {
        if self.enabled {
            let invalid: bool = self.conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_foreign_key_check)",
                [],
                |row| row.get(0),
            )?;
            if invalid {
                return Err(DbError::Migration(
                    "storage object references are invalid".into(),
                ));
            }
        }
        Ok(())
    }
}

impl Drop for StorageRebuild<'_> {
    fn drop(&mut self) {
        if self.enabled {
            if !self.conn.is_autocommit() {
                self.conn
                    .execute_batch("ROLLBACK")
                    .expect("release failed storage rebuild transaction");
            }
            self.conn
                .execute_batch("PRAGMA foreign_keys=ON")
                .expect("restore migration foreign keys");
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/storage_versions_migration.rs"]
mod tests;
