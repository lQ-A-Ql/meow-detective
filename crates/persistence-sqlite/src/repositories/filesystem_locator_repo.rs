use crate::connection::{DbError, DbResult};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

const DIRECTORY_LOCATOR_KEY_PREFIX: &str = "filesystem_directory_locators:v2:";
const FILE_LOCATOR_KEY_PREFIX: &str = "filesystem_file_locators:v2:";
const MAX_DIRECTORY_LOCATORS: usize = 100_000;
const MAX_FILE_LOCATORS: usize = 150_000;
const MAX_DIRECTORY_LOCATOR_BYTES: usize = 32 * 1024 * 1024;
const MAX_FILE_LOCATOR_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesystemLocatorRecord {
    pub path: String,
    pub locator: String,
}

pub type FilesystemDirectoryLocatorRecord = FilesystemLocatorRecord;
pub type FilesystemFileLocatorRecord = FilesystemLocatorRecord;

pub struct FilesystemLocatorRepo<'a> {
    conn: &'a Connection,
}

impl<'a> FilesystemLocatorRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn replace_directory_locators(
        &self,
        data_source_id: &str,
        partition_index: usize,
        filesystem_kind: &str,
        scope_identity: &str,
        locators: &[FilesystemDirectoryLocatorRecord],
    ) -> DbResult<()> {
        validate_identity(data_source_id, filesystem_kind, scope_identity)?;
        let key = locator_key(
            DIRECTORY_LOCATOR_KEY_PREFIX,
            data_source_id,
            partition_index,
            filesystem_kind,
            scope_identity,
        );
        self.replace_locators(
            key,
            locators,
            MAX_DIRECTORY_LOCATORS,
            MAX_DIRECTORY_LOCATOR_BYTES,
            "directory",
        )
    }

    pub fn list_directory_locators(
        &self,
        data_source_id: &str,
        partition_index: usize,
        filesystem_kind: &str,
        scope_identity: &str,
    ) -> DbResult<Vec<FilesystemDirectoryLocatorRecord>> {
        validate_identity(data_source_id, filesystem_kind, scope_identity)?;
        let key = locator_key(
            DIRECTORY_LOCATOR_KEY_PREFIX,
            data_source_id,
            partition_index,
            filesystem_kind,
            scope_identity,
        );
        self.list_locators(
            key,
            MAX_DIRECTORY_LOCATORS,
            MAX_DIRECTORY_LOCATOR_BYTES,
            "directory",
        )
    }

    pub fn replace_file_locators(
        &self,
        data_source_id: &str,
        partition_index: usize,
        filesystem_kind: &str,
        scope_identity: &str,
        locators: &[FilesystemFileLocatorRecord],
    ) -> DbResult<()> {
        validate_identity(data_source_id, filesystem_kind, scope_identity)?;
        let key = locator_key(
            FILE_LOCATOR_KEY_PREFIX,
            data_source_id,
            partition_index,
            filesystem_kind,
            scope_identity,
        );
        self.replace_locators(
            key,
            locators,
            MAX_FILE_LOCATORS,
            MAX_FILE_LOCATOR_BYTES,
            "file",
        )
    }

    pub fn list_file_locators(
        &self,
        data_source_id: &str,
        partition_index: usize,
        filesystem_kind: &str,
        scope_identity: &str,
    ) -> DbResult<Vec<FilesystemFileLocatorRecord>> {
        validate_identity(data_source_id, filesystem_kind, scope_identity)?;
        let key = locator_key(
            FILE_LOCATOR_KEY_PREFIX,
            data_source_id,
            partition_index,
            filesystem_kind,
            scope_identity,
        );
        self.list_locators(key, MAX_FILE_LOCATORS, MAX_FILE_LOCATOR_BYTES, "file")
    }

    fn replace_locators(
        &self,
        key: String,
        locators: &[FilesystemLocatorRecord],
        max_count: usize,
        max_bytes: usize,
        kind: &str,
    ) -> DbResult<()> {
        validate_locators(locators, max_count, kind)?;
        if locators.is_empty() {
            self.conn
                .execute("DELETE FROM source_meta WHERE key = ?1", [key])?;
            return Ok(());
        }
        let encoded = serde_json::to_string(locators).map_err(|error| {
            DbError::System(format!("encode filesystem {kind} locators: {error}"))
        })?;
        if encoded.len() > max_bytes {
            return Err(DbError::System(format!(
                "filesystem {kind} locator payload exceeds the persistence limit"
            )));
        }
        self.conn.execute(
            "INSERT INTO source_meta (key, value)
             VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, encoded],
        )?;
        Ok(())
    }

    fn list_locators(
        &self,
        key: String,
        max_count: usize,
        max_bytes: usize,
        kind: &str,
    ) -> DbResult<Vec<FilesystemLocatorRecord>> {
        let encoded = self
            .conn
            .query_row(
                "SELECT value FROM source_meta WHERE key = ?1",
                [key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(encoded) = encoded else {
            return Ok(Vec::new());
        };
        if encoded.len() > max_bytes {
            return Err(DbError::System(format!(
                "stored filesystem {kind} locator payload exceeds the limit"
            )));
        }
        let locators =
            serde_json::from_str::<Vec<FilesystemLocatorRecord>>(&encoded).map_err(|error| {
                DbError::System(format!("decode filesystem {kind} locators: {error}"))
            })?;
        validate_locators(&locators, max_count, kind)?;
        Ok(locators)
    }
}

fn validate_identity(
    data_source_id: &str,
    filesystem_kind: &str,
    scope_identity: &str,
) -> DbResult<()> {
    if !valid_token(data_source_id) {
        return Err(DbError::System(
            "filesystem locator data-source ID is invalid".to_string(),
        ));
    }
    if !valid_token(filesystem_kind) {
        return Err(DbError::System(
            "filesystem locator kind is invalid".to_string(),
        ));
    }
    if scope_identity.len() != 64
        || !scope_identity
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(DbError::System(
            "filesystem locator scope identity is invalid".to_string(),
        ));
    }
    Ok(())
}

fn validate_locators(
    locators: &[FilesystemLocatorRecord],
    max_count: usize,
    kind: &str,
) -> DbResult<()> {
    if locators.len() > max_count {
        return Err(DbError::System(format!(
            "filesystem {kind} locator count exceeds the limit"
        )));
    }
    let mut previous_path: Option<&str> = None;
    for locator in locators {
        if locator.path.is_empty()
            || locator.path.contains('\0')
            || locator.locator.is_empty()
            || locator.locator.contains('\0')
        {
            return Err(DbError::System(format!(
                "filesystem {kind} locator contains an invalid field"
            )));
        }
        if previous_path.is_some_and(|previous| previous >= locator.path.as_str()) {
            return Err(DbError::System(format!(
                "filesystem {kind} locators must be strictly path-sorted"
            )));
        }
        previous_path = Some(&locator.path);
    }
    Ok(())
}

fn valid_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn locator_key(
    prefix: &str,
    data_source_id: &str,
    partition_index: usize,
    filesystem_kind: &str,
    scope_identity: &str,
) -> String {
    let kind = filesystem_kind.to_ascii_lowercase();
    format!(
        "{prefix}{}:{data_source_id}:{partition_index}:{}:{kind}:{scope_identity}",
        data_source_id.len(),
        kind.len()
    )
}
