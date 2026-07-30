use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::tantivy_writer::{IndexError, Result};

const IDENTITY_FILE_NAME: &str = ".meow-search-generation.json";
const IDENTITY_FORMAT_VERSION: u32 = 1;
const SEARCH_SCHEMA_VERSION: u32 = 3;
const MAX_IDENTITY_BYTES: u64 = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SearchIndexIdentity {
    format_version: u32,
    schema_version: u32,
    generation: String,
}

impl SearchIndexIdentity {
    pub(super) fn create(index_dir: &Path) -> Result<Self> {
        let identity = Self {
            format_version: IDENTITY_FORMAT_VERSION,
            schema_version: SEARCH_SCHEMA_VERSION,
            generation: Uuid::new_v4().to_string(),
        };
        identity.persist(index_dir)?;
        Ok(identity)
    }

    pub(super) fn load(index_dir: &Path) -> Result<Self> {
        let path = identity_path(index_dir);
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() > MAX_IDENTITY_BYTES
        {
            return Err(identity_error(
                "identity metadata is not a bounded regular file",
            ));
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        OpenOptions::new()
            .read(true)
            .open(path)?
            .take(MAX_IDENTITY_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_IDENTITY_BYTES {
            return Err(identity_error("identity metadata exceeds the size limit"));
        }
        let identity: Self = serde_json::from_slice(&bytes)
            .map_err(|error| identity_error(format!("invalid identity metadata: {error}")))?;
        identity.validate()?;
        Ok(identity)
    }

    pub(super) fn generation(&self) -> &str {
        &self.generation
    }

    pub(super) fn schema_version(&self) -> u32 {
        self.schema_version
    }

    fn persist(&self, index_dir: &Path) -> Result<()> {
        let destination = identity_path(index_dir);
        let temporary = temporary_identity_path(index_dir);
        let bytes = serde_json::to_vec(self)
            .map_err(|error| identity_error(format!("serialize identity metadata: {error}")))?;
        let write_result = (|| {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, &destination)?;
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        write_result
    }

    fn validate(&self) -> Result<()> {
        if self.format_version != IDENTITY_FORMAT_VERSION {
            return Err(identity_error("unsupported identity format version"));
        }
        if self.schema_version != SEARCH_SCHEMA_VERSION {
            return Err(identity_error("unsupported search schema version"));
        }
        let parsed = Uuid::parse_str(&self.generation)
            .map_err(|_| identity_error("generation is not a canonical UUID"))?;
        if parsed.to_string() != self.generation {
            return Err(identity_error("generation is not a canonical UUID"));
        }
        Ok(())
    }
}

fn identity_path(index_dir: &Path) -> PathBuf {
    index_dir.join(IDENTITY_FILE_NAME)
}

fn temporary_identity_path(index_dir: &Path) -> PathBuf {
    index_dir.join(format!("{IDENTITY_FILE_NAME}.{}.tmp", Uuid::new_v4()))
}

fn identity_error(message: impl Into<String>) -> IndexError {
    IndexError::Identity(message.into())
}
