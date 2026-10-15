//! Durable, source-local index deferral for a fresh file catalog.
use super::source_bulk_load_repo::{BulkLoadState, SourceBulkLoadRepo};
use crate::{DbError, DbResult};
use rusqlite::Connection;

const DEFINITIONS: &str = "bulk_load.indexes";

impl SourceBulkLoadRepo {
    /// Only defer ordinary secondary indexes on an empty catalog. Primary and
    /// unique indexes, and parent/source lookups required during ingest, stay live.
    pub fn begin(conn: &Connection, attempt: &str) -> DbResult<bool> {
        Self::recover(conn)?;
        let rows: i64 =
            conn.query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))?;
        if rows != 0 {
            return Ok(false);
        }
        let definitions = conn
            .prepare(
                "SELECT name, sql FROM sqlite_master WHERE type='index' AND tbl_name='file_entries'
             AND sql IS NOT NULL AND name NOT IN ('idx_source_file_entries_parent',
                 'idx_source_file_entries_data_source')
             AND name IN (SELECT name FROM pragma_index_list('file_entries') WHERE \"unique\"=0)
             ORDER BY name",
            )?
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let serialized =
            serde_json::to_string(&definitions).map_err(|e| DbError::System(e.to_string()))?;
        let transaction = conn.unchecked_transaction()?;
        transaction.execute(
            "INSERT OR REPLACE INTO source_meta(key,value) VALUES (?1,?2)",
            rusqlite::params![DEFINITIONS, serialized],
        )?;
        for (name, _) in definitions {
            transaction.execute_batch(&format!("DROP INDEX {}", quote_identifier(&name)))?;
        }
        transaction.execute(
            "INSERT OR REPLACE INTO source_meta(key,value) VALUES ('bulk_load.state','active')",
            [],
        )?;
        transaction.execute(
            "INSERT OR REPLACE INTO source_meta(key,value) VALUES ('bulk_load.attempt',?1)",
            [attempt],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    /// Rebuilding is idempotent after process termination. Do not publish ready
    /// until every saved index and ANALYZE have committed successfully.
    pub fn finish(conn: &Connection) -> DbResult<()> {
        if Self::state(conn)? == BulkLoadState::Inactive {
            return Ok(());
        }
        use rusqlite::OptionalExtension;
        let saved: Option<String> = conn
            .query_row(
                "SELECT value FROM source_meta WHERE key=?1",
                [DEFINITIONS],
                |row| row.get(0),
            )
            .optional()?;
        let saved = saved.ok_or_else(|| {
            DbError::System("Missing deferred index manifest; import recovery required".into())
        })?;
        let definitions: Vec<(String, String)> = serde_json::from_str(&saved)
            .map_err(|e| DbError::System(format!("Invalid deferred index manifest: {e}")))?;
        conn.execute(
            "UPDATE source_meta SET value='rebuilding' WHERE key='bulk_load.state'",
            [],
        )?;
        let transaction = conn.unchecked_transaction()?;
        for (name, sql) in definitions {
            let exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='index' AND name=?1)",
                [&name],
                |row| row.get(0),
            )?;
            if !exists {
                if !sql
                    .trim_start()
                    .to_ascii_uppercase()
                    .starts_with("CREATE INDEX ")
                {
                    return Err(DbError::System("Invalid deferred index definition".into()));
                }
                transaction.execute(&sql, [])?;
            }
            let actual: (String, String) = transaction.query_row(
                "SELECT tbl_name, sql FROM sqlite_master WHERE type='index' AND name=?1",
                [&name],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if actual != ("file_entries".to_string(), sql) {
                return Err(DbError::System(
                    "Deferred index definition differs from its manifest".into(),
                ));
            }
        }
        transaction.execute_batch("ANALYZE file_entries")?;
        transaction.execute(
            "UPDATE source_meta SET value='ready' WHERE key='bulk_load.state'",
            [],
        )?;
        transaction.commit()?;
        Self::clear_transient(conn)
    }

    pub fn recover(conn: &Connection) -> DbResult<()> {
        match Self::state(conn)? {
            BulkLoadState::Inactive => Ok(()),
            _ => Self::finish(conn),
        }
    }
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}
