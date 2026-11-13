use super::identity::{self, DirectoryIdentity};
use domain::DataSourceId;
use persistence_sqlite::{DbError, DbResult};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

const PREFIX: &str = ".analysis-gc-";
const MAX_INTENT_BYTES: u64 = 8 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Intent {
    version: u32,
    source_id: String,
    original: String,
    quarantine: String,
    manifest_digest: String,
    identity: DirectoryIdentity,
}

pub(super) struct PendingIntent {
    pub path: PathBuf,
    pub value: Intent,
}

impl PendingIntent {
    pub(super) fn original(&self, directory: &Path) -> PathBuf {
        directory.join(&self.value.original)
    }

    pub(super) fn quarantine(&self, directory: &Path) -> PathBuf {
        directory.join(&self.value.quarantine)
    }

    pub(super) fn matches(&self, path: &Path) -> DbResult<bool> {
        Ok(identity::read(path)? == self.value.identity)
    }
}

pub(super) fn is_intent_name(name: &str) -> bool {
    name.strip_prefix(PREFIX)
        .and_then(|value| value.strip_suffix(".json"))
        .is_some_and(|value| uuid::Uuid::parse_str(value).is_ok())
}

pub(super) fn pending(
    directory: &Path,
    source: &DataSourceId,
) -> DbResult<(Vec<PendingIntent>, bool)> {
    let mut intents = Vec::new();
    for (index, entry) in std::fs::read_dir(directory)?.enumerate() {
        if index == super::MAX_DIRECTORY_ENTRIES {
            return Ok((intents, true));
        }
        let entry = entry?;
        if !is_intent_name(&entry.file_name().to_string_lossy()) {
            continue;
        }
        if let Some(intent) = load(&entry.path(), source)? {
            intents.push(intent);
            if intents.len() == super::MAX_RECLAIMED_GENERATIONS {
                return Ok((intents, true));
            }
        }
    }
    intents.sort_by(|left, right| left.path.cmp(&right.path));
    Ok((intents, false))
}

pub(super) fn load(path: &Path, source: &DataSourceId) -> DbResult<Option<PendingIntent>> {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return Ok(None);
    };
    if !is_intent_name(name) {
        return Ok(None);
    }
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || identity::linked(&metadata) || metadata.len() > MAX_INTENT_BYTES {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_INTENT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    let Ok(value) = serde_json::from_slice::<Intent>(&bytes) else {
        return Ok(None);
    };
    let uuid = name.trim_start_matches(PREFIX).trim_end_matches(".json");
    let expected = format!("{PREFIX}{uuid}.removing");
    if value.version != 1
        || value.source_id != source.0
        || value.quarantine != expected
        || !super::is_analysis_name(&value.original)
        || value.manifest_digest.len() != 64
        || !value
            .manifest_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Ok(None);
    }
    Ok(Some(PendingIntent {
        path: path.to_owned(),
        value,
    }))
}

pub(super) fn create(
    directory: &Path,
    original: &Path,
    source: &DataSourceId,
) -> DbResult<PendingIntent> {
    let original_name = original
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| super::is_analysis_name(name))
        .ok_or_else(|| DbError::System("cleanup generation name is invalid".into()))?;
    let manifest_digest = persistence_fjall::source_generation_manifest_digest(original)
        .map_err(|error| DbError::System(error.to_string()))?;
    let id = uuid::Uuid::new_v4();
    let path = directory.join(format!("{PREFIX}{id}.json"));
    let value = Intent {
        version: 1,
        source_id: source.0.clone(),
        original: original_name.into(),
        quarantine: format!("{PREFIX}{id}.removing"),
        manifest_digest,
        identity: identity::read(original)?,
    };
    let bytes = serde_json::to_vec(&value).map_err(|error| DbError::System(error.to_string()))?;
    let temporary = path.with_extension("json.new");
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    identity::rename(&temporary, &path)?;
    Ok(PendingIntent { path, value })
}

pub(super) fn finish(intent: PendingIntent) -> DbResult<()> {
    std::fs::remove_file(&intent.path)?;
    identity::sync_parent(&intent.path)
}
