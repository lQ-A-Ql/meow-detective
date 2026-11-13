use super::super::CorrelationError;
use super::cache::CachedSnapshot;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use transport::dto::ArtifactRowDto;

pub(super) fn matches_revision(cached: &CachedSnapshot, revision: &str) -> bool {
    cached
        .artifact_hash
        .strip_prefix(revision)
        .is_some_and(|tail| tail.starts_with('|'))
}

pub(super) fn input_key(
    revision: &str,
    artifacts: &[ArtifactRowDto],
) -> Result<String, CorrelationError> {
    let versions = versions(artifacts)?;
    let encoded = serde_json::to_string(&versions)
        .map_err(|error| CorrelationError::Other(error.to_string()))?;
    Ok(format!("{revision}|{encoded}"))
}

pub(super) fn append_only(
    cached: &CachedSnapshot,
    revision: &str,
    artifacts: &[ArtifactRowDto],
) -> Result<bool, CorrelationError> {
    let (old_revision, encoded) = cached.artifact_hash.split_once('|').ok_or_else(|| {
        CorrelationError::Other("correlation cache input record is malformed".to_owned())
    })?;
    if old_revision
        .split(':')
        .skip(2)
        .ne(revision.split(':').skip(2))
    {
        return Ok(false);
    }
    let old: BTreeMap<String, String> = serde_json::from_str(encoded)
        .map_err(|error| CorrelationError::Other(error.to_string()))?;
    let current = versions(artifacts)?;
    Ok(old.len() < current.len() && old.iter().all(|(id, hash)| current.get(id) == Some(hash)))
}

fn versions(artifacts: &[ArtifactRowDto]) -> Result<BTreeMap<String, String>, CorrelationError> {
    artifacts
        .iter()
        .map(|artifact| {
            let bytes = serde_json::to_vec(artifact)
                .map_err(|error| CorrelationError::Other(error.to_string()))?;
            Ok((artifact.id.clone(), format!("{:x}", Sha256::digest(bytes))))
        })
        .collect()
}
