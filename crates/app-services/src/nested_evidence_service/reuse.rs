use super::materialize::MaterializeError;
use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

/// Retry only a source whose complete materialized bytes still match its ledger.
pub(super) fn verify_existing(
    conn: &rusqlite::Connection,
    root: &Path,
    case: &CaseId,
    id: &DataSourceId,
    size: u64,
    cancel: &AtomicBool,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<bool, MaterializeError> {
    let source = DataSourceRepo::new(conn)
        .find_by_case(case)?
        .into_iter()
        .find(|source| source.id == *id);
    let Some(source) = source else {
        return Ok(false);
    };
    let expected = root
        .join("derived")
        .join("nested-evidence")
        .join(format!("{}.vhd", id.0));
    if source.source_path != expected || source.provenance.evidence_size != Some(size) {
        return Err(MaterializeError::Conflict);
    }
    let metadata = std::fs::symlink_metadata(&expected)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || !expected.canonicalize()?.starts_with(root.canonicalize()?)
    {
        return Err(MaterializeError::Conflict);
    }
    let mut file = std::fs::File::open(&expected)?;
    if file.metadata()?.len() != size {
        return Err(MaterializeError::Conflict);
    }
    let hash = crate::hash_service::HashService::digest_reader(
        &mut file,
        infrastructure::hashing::HashAlgorithm::Sha256,
        || cancel.load(Ordering::Acquire),
        |bytes| progress(bytes, size),
    )?
    .ok_or(MaterializeError::Cancelled)?;
    if source.provenance.source_hash_sha256.as_deref() != Some(&hash) {
        return Err(MaterializeError::Conflict);
    }
    Ok(true)
}
