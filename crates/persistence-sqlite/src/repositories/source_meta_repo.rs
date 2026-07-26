use crate::connection::{DbError, DbResult};
use rusqlite::{types::ValueRef, Connection};

const MAX_KEY_BYTES: usize = 256;
const MAX_VALUE_BYTES: usize = 16 * 1024 * 1024;
const MAX_REVISION_CAS_ATTEMPTS: usize = 16;

pub const ARTIFACT_CURSOR_REVISION_KEY: &str = "cursor-revision:artifacts:v1";
pub const TIMELINE_CURSOR_REVISION_KEY: &str = "cursor-revision:timeline:v1";

pub struct SourceMetaRepo<'a> {
    conn: &'a Connection,
}

impl<'a> SourceMetaRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn read(&self, key: &str) -> DbResult<Option<String>> {
        validate_key(key)?;

        let mut statement = self
            .conn
            .prepare("SELECT value FROM source_meta WHERE key = ?1")?;
        let mut rows = statement.query([key])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };

        let value = match row.get_ref(0)? {
            ValueRef::Text(value) => value,
            ValueRef::Blob(_) => {
                return Err(DbError::System(
                    "source_meta value is not stored as text".to_string(),
                ));
            }
            ValueRef::Null | ValueRef::Integer(_) | ValueRef::Real(_) => {
                return Err(DbError::System(
                    "source_meta value has an invalid SQLite type".to_string(),
                ));
            }
        };
        if value.len() > MAX_VALUE_BYTES {
            return Err(DbError::System(
                "source_meta value exceeds the persistence limit".to_string(),
            ));
        }

        String::from_utf8(value.to_vec())
            .map(Some)
            .map_err(|_| DbError::System("source_meta value is not valid UTF-8".to_string()))
    }

    pub fn read_revision(&self, key: &str) -> DbResult<u64> {
        if !self.table_available()? {
            return Ok(0);
        }
        self.read_revision_value(key)
    }

    pub fn bump_revision(&self, key: &str) -> DbResult<u64> {
        if !self.table_available()? {
            return Ok(0);
        }

        for _ in 0..MAX_REVISION_CAS_ATTEMPTS {
            let Some(stored) = self.read(key)? else {
                let inserted = self.conn.execute(
                    "INSERT INTO source_meta (key, value) VALUES (?1, '1')
                     ON CONFLICT(key) DO NOTHING",
                    [key],
                )?;
                if inserted == 1 {
                    return Ok(1);
                }
                continue;
            };
            let revision = parse_revision_value(key, &stored)?
                .checked_add(1)
                .ok_or_else(|| {
                    DbError::System(format!("source_meta revision '{key}' overflowed"))
                })?;
            let updated = self.conn.execute(
                "UPDATE source_meta SET value = ?1 WHERE key = ?2 AND value = ?3",
                rusqlite::params![revision.to_string(), key, stored],
            )?;
            if updated == 1 {
                return Ok(revision);
            }
        }

        Err(DbError::System(format!(
            "source_meta revision '{key}' changed too frequently"
        )))
    }

    fn read_revision_value(&self, key: &str) -> DbResult<u64> {
        let Some(value) = self.read(key)? else {
            return Ok(0);
        };
        parse_revision_value(key, &value)
    }

    fn table_available(&self) -> DbResult<bool> {
        self.conn
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM sqlite_schema
                    WHERE type = 'table' AND name = 'source_meta'
                )",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }
}

fn parse_revision_value(key: &str, value: &str) -> DbResult<u64> {
    value.parse::<u64>().map_err(|_| {
        DbError::System(format!(
            "source_meta revision '{key}' is not a valid unsigned integer"
        ))
    })
}

fn validate_key(key: &str) -> DbResult<()> {
    if key.is_empty() {
        return Err(DbError::System(
            "source_meta key must not be empty".to_string(),
        ));
    }
    if key.len() > MAX_KEY_BYTES {
        return Err(DbError::System(
            "source_meta key exceeds the persistence limit".to_string(),
        ));
    }
    Ok(())
}
