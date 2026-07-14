use rusqlite::Connection;
use transport::CommandError;

use super::{spool_error, SPOOL_SCHEMA_VERSION};

pub(super) fn configure_connection(connection: &Connection) -> Result<(), CommandError> {
    connection
        .execute_batch(
            "PRAGMA page_size=65536;
             PRAGMA journal_mode=OFF;
             PRAGMA synchronous=OFF;
             PRAGMA temp_store=MEMORY;
             PRAGMA locking_mode=EXCLUSIVE;
             PRAGMA cache_size=-262144;
             PRAGMA cache_spill=OFF;
             PRAGMA foreign_keys=ON;",
        )
        .map_err(CommandError::from_service_error)
}

pub(super) fn create_schema(connection: &Connection) -> Result<(), CommandError> {
    connection
        .execute_batch(
            "CREATE TABLE spool_meta (
                schema_version INTEGER NOT NULL CHECK (schema_version = 1)
             );
             INSERT INTO spool_meta(schema_version) VALUES (1);
             CREATE TABLE point_mutations (
                column_family_id INTEGER NOT NULL CHECK (column_family_id >= 0),
                user_key BLOB NOT NULL,
                sequence INTEGER NOT NULL
                    CHECK (sequence BETWEEN 0 AND 72057594037927935),
                value_type INTEGER NOT NULL CHECK (value_type IN (0, 1, 2, 7)),
                value BLOB NOT NULL,
                source_kind INTEGER NOT NULL CHECK (source_kind IN (0, 1)),
                file_number INTEGER NOT NULL CHECK (file_number > 0),
                level INTEGER CHECK (level IS NULL OR level >= 0),
                physical_offset INTEGER NOT NULL CHECK (physical_offset >= 0),
                primary_ordinal INTEGER NOT NULL CHECK (primary_ordinal >= 0),
                secondary_ordinal INTEGER NOT NULL CHECK (secondary_ordinal >= 0),
                PRIMARY KEY (
                    column_family_id,
                    user_key,
                    sequence DESC
                )
             ) WITHOUT ROWID;
             CREATE TABLE range_tombstones (
                column_family_id INTEGER NOT NULL CHECK (column_family_id >= 0),
                start_key BLOB NOT NULL,
                end_key BLOB NOT NULL,
                sequence INTEGER NOT NULL
                    CHECK (sequence BETWEEN 0 AND 72057594037927935),
                source_kind INTEGER NOT NULL CHECK (source_kind IN (0, 1)),
                file_number INTEGER NOT NULL CHECK (file_number > 0),
                level INTEGER CHECK (level IS NULL OR level >= 0),
                physical_offset INTEGER NOT NULL CHECK (physical_offset >= 0),
                primary_ordinal INTEGER NOT NULL CHECK (primary_ordinal >= 0),
                secondary_ordinal INTEGER NOT NULL CHECK (secondary_ordinal >= 0),
                CHECK (start_key <= end_key),
                PRIMARY KEY (column_family_id, start_key, sequence DESC)
             ) WITHOUT ROWID;",
        )
        .map_err(CommandError::from_service_error)?;
    let version: u32 = connection
        .query_row("SELECT schema_version FROM spool_meta", [], |row| {
            row.get(0)
        })
        .map_err(CommandError::from_service_error)?;
    if version != SPOOL_SCHEMA_VERSION {
        return Err(spool_error("recovery spool schema version mismatch"));
    }
    Ok(())
}
