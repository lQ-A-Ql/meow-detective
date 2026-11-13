use crate::control::connections::try_begin_generation_gc;
use domain::DataSourceId;
use persistence_sqlite::{DbError, DbResult};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

mod identity;
mod intent;
mod retired;

const MAX_DIRECTORY_ENTRIES: usize = 4_096;
const MAX_RECLAIMED_GENERATIONS: usize = 8;
const PREFIX: &str = "source.fjall.build.analysis-";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AnalysisGenerationGcOutcome {
    pub removed: usize,
    pub pending: bool,
}

pub fn reclaim_analysis_generations(
    source_directory: &Path,
) -> DbResult<Option<AnalysisGenerationGcOutcome>> {
    if !source_directory.exists() {
        return Ok(Some(AnalysisGenerationGcOutcome::default()));
    }
    let directory = std::fs::canonicalize(source_directory)?;
    let source = source_identity(&directory)?;
    let Some(guard) = try_begin_generation_gc(&directory)? else {
        return Ok(None);
    };
    let current = guard.current_generation_root(&source)?;
    let (intents, pending) = intent::pending(&directory, &source)?;
    let reserved = intents
        .iter()
        .map(|intent| intent.original(&directory))
        .collect::<HashSet<_>>();
    let mut outcome = AnalysisGenerationGcOutcome {
        pending,
        ..Default::default()
    };
    let mut attempts = 0;
    for intent in intents {
        attempts += 1;
        if retired::resume(&guard, &directory, &current, intent)? {
            outcome.removed += 1;
        } else {
            outcome.pending = true;
        }
    }
    if attempts == MAX_RECLAIMED_GENERATIONS {
        return Ok(Some(outcome));
    }
    let (candidates, more) = candidates(
        &directory,
        &current,
        &source,
        &reserved,
        MAX_RECLAIMED_GENERATIONS - attempts,
    )?;
    outcome.pending |= more;
    for path in candidates {
        if retired::start(&guard, &directory, &current, &path, &source)? {
            outcome.removed += 1;
        } else {
            outcome.pending = true;
        }
    }
    Ok(Some(outcome))
}

fn candidates(
    directory: &Path,
    current: &Path,
    source: &DataSourceId,
    reserved: &HashSet<PathBuf>,
    limit: usize,
) -> DbResult<(Vec<PathBuf>, bool)> {
    let mut candidates = Vec::new();
    for (index, entry) in std::fs::read_dir(directory)?.enumerate() {
        if index == MAX_DIRECTORY_ENTRIES {
            return Err(DbError::System(
                "source generation directory exceeds reclamation scan limit".into(),
            ));
        }
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_analysis_name(&name) {
            continue;
        }
        let uuid = name.strip_prefix(PREFIX).expect("validated analysis name");
        let metadata = std::fs::symlink_metadata(entry.path())?;
        if !metadata.is_dir() || identity::linked(&metadata) {
            continue;
        }
        let path = std::fs::canonicalize(entry.path())?;
        if path.parent() != Some(directory) || path == current || reserved.contains(&path) {
            continue;
        }
        let Ok(manifest) =
            persistence_fjall::read_source_generation_manifest_at_path(&path, &source.0)
        else {
            continue;
        };
        if manifest.attempt_id != name && manifest.attempt_id != format!("analysis-{uuid}") {
            continue;
        }
        candidates.push(path);
        if candidates.len() == limit {
            candidates.sort();
            return Ok((candidates, true));
        }
    }
    candidates.sort();
    Ok((candidates, false))
}

fn is_analysis_name(name: &str) -> bool {
    name.strip_prefix(PREFIX)
        .is_some_and(|value| uuid::Uuid::parse_str(value).is_ok())
}

fn source_identity(directory: &Path) -> DbResult<DataSourceId> {
    let source = DataSourceId(
        directory
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| DbError::System("source directory has no valid identity".into()))?
            .to_owned(),
    );
    crate::control::identity::validate_source_storage_id(&source)?;
    Ok(source)
}

pub fn has_pending_analysis_gc(source_directory: &Path) -> DbResult<bool> {
    if !source_directory.try_exists()? {
        return Ok(false);
    }
    let directory = std::fs::canonicalize(source_directory)?;
    let source = source_identity(&directory)?;
    if !intent::pending(&directory, &source)?.0.is_empty() {
        return Ok(true);
    }
    let case_root = directory
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| DbError::System("source directory has no case root".into()))?;
    let current = crate::control::current_source_generation_root(case_root, &source)?;
    Ok(
        !candidates(&directory, &current, &source, &HashSet::new(), 1)?
            .0
            .is_empty(),
    )
}

#[cfg(test)]
#[path = "../../tests/unit/analysis_gc_intent.rs"]
mod tests;
