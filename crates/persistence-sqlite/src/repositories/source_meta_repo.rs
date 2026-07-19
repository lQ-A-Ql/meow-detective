use crate::connection::{DbError, DbResult};
use rusqlite::{types::ValueRef, Connection};

const MAX_KEY_BYTES: usize = 256;
const MAX_VALUE_BYTES: usize = 16 * 1024 * 1024;

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
