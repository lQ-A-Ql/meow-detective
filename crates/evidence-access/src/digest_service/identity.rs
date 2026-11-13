//! Evidence revision derived from registered identity and bounded source metadata.

use super::{DigestServiceError, DigestTarget};
use domain::{CaseId, DataSourceId, DataSourceKind, FileEntryId};
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

pub struct DigestIdentity {
    pub source_id: String,
    pub file_id: Option<String>,
    pub revision: String,
}

pub fn snapshot(
    conn: &rusqlite::Connection,
    root: &Path,
    case: &CaseId,
    target: &DigestTarget<'_>,
) -> Result<DigestIdentity, DigestServiceError> {
    if let Some(id) = target.file_id {
        let global = crate::source_db::GlobalFileId::parse(&FileEntryId(id.to_owned()))
            .map_err(|_| DigestServiceError::InvalidInput)?;
        if target
            .data_source_id
            .is_some_and(|source| source != global.data_source_id.0)
        {
            return Err(DigestServiceError::InvalidInput);
        }
        let id = global.encode().0;
        return snapshot_files(conn, root, case, &global.data_source_id, &[&id])?
            .remove(&id)
            .ok_or(DigestServiceError::NotFound);
    }
    let source_id = DataSourceId(
        target
            .data_source_id
            .ok_or(DigestServiceError::InvalidInput)?
            .to_owned(),
    );
    let (hasher, _) = source_base(conn, case, &source_id, true)?;
    Ok(DigestIdentity {
        source_id: source_id.0,
        file_id: None,
        revision: hex::encode(hasher.finalize()),
    })
}

pub fn snapshot_files(
    conn: &rusqlite::Connection,
    root: &Path,
    case: &CaseId,
    source_id: &DataSourceId,
    ids: &[&str],
) -> Result<BTreeMap<String, DigestIdentity>, DigestServiceError> {
    if ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    if ids.len() > 500 {
        return Err(DigestServiceError::InvalidInput);
    }
    let files = ids
        .iter()
        .map(|id| crate::source_db::GlobalFileId::parse(&FileEntryId((*id).to_owned())))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| DigestServiceError::InvalidInput)?;
    if files.iter().any(|file| file.data_source_id != *source_id) {
        return Err(DigestServiceError::InvalidInput);
    }
    let source = crate::source_db::open_ready_source_read_only_by_id(conn, root, case, source_id)
        .map_err(crate::file_service::FileServiceError::from)?;
    let local_ids = files
        .iter()
        .map(|file| file.local_id.clone())
        .collect::<Vec<_>>();
    let entries = FileRepo::new(&source.connection).find_by_ids(&local_ids)?;
    let (base, kind) = source_base(conn, case, source_id, false)?;
    let mut context = crate::file_service::SourceReadContext::new(
        &source.connection,
        conn,
        root,
        case,
        source_id,
    );
    let mut identities = BTreeMap::new();
    for file in files {
        let entry = entries
            .get(&file.local_id.0)
            .ok_or(DigestServiceError::NotFound)?;
        let mut hasher = base.clone();
        hasher.update(b"file\0");
        hasher.update(entry.id.0.as_bytes());
        hasher.update(entry.path.as_bytes());
        hasher.update(entry.size.unwrap_or_default().to_le_bytes());
        hasher.update(
            format!(
                "{:?}:{:?}:{:?}",
                entry.entry_type, entry.modified_at, entry.changed_at
            )
            .as_bytes(),
        );
        if kind == DataSourceKind::LogicalDirectory {
            let path =
                crate::file_service::get_file_path_for_entry(&mut context, &file.local_id.0)?;
            hash_metadata(&mut hasher, &path)?;
        }
        let canonical = file.encode().0;
        identities.insert(
            canonical.clone(),
            DigestIdentity {
                source_id: source_id.0.clone(),
                file_id: Some(canonical),
                revision: hex::encode(hasher.finalize()),
            },
        );
    }
    Ok(identities)
}

fn source_base(
    conn: &rusqlite::Connection,
    case: &CaseId,
    source_id: &DataSourceId,
    include_tree: bool,
) -> Result<(Sha256, DataSourceKind), DigestServiceError> {
    let source = DataSourceRepo::new(conn)
        .find_by_case(case)?
        .into_iter()
        .find(|source| source.id == *source_id)
        .ok_or(DigestServiceError::NotFound)?;
    let mut hasher = Sha256::new();
    hasher.update(b"evidence-revision-v2\0");
    hasher.update(case.0.as_bytes());
    hasher.update(source.id.0.as_bytes());
    hasher.update(format!("{:?}", source.kind).as_bytes());
    if let Some(storage) = DataSourceRepo::new(conn).find_storage(source_id)? {
        hasher.update(storage.source_db_rel_path.unwrap_or_default().as_bytes());
        hasher.update(storage.schema_version.unwrap_or_default().as_bytes());
    }
    hash_metadata(&mut hasher, &source.source_path)?;
    if include_tree && source.source_path.is_dir() {
        hash_tree(&mut hasher, &source.source_path)?;
    }
    if source.kind == DataSourceKind::Raw {
        let reader = evidence_core::RawImageReader::open(&source.source_path).map_err(io_error)?;
        for parent in reader.backing_paths() {
            hash_metadata(&mut hasher, parent)?;
        }
    }
    if source.kind == DataSourceKind::E01 {
        for part in crate::hash_service::volumes::discover_e01_segments(&source.source_path)
            .map_err(io_error)?
        {
            hash_metadata(&mut hasher, &part)?;
        }
    }
    Ok((hasher, source.kind))
}

fn hash_metadata(hash: &mut Sha256, path: &Path) -> Result<(), DigestServiceError> {
    let metadata = std::fs::metadata(path).map_err(io_error)?;
    hash.update(path.as_os_str().to_string_lossy().as_bytes());
    hash.update(metadata.len().to_le_bytes());
    let modified = metadata.modified().map_err(io_error)?;
    hash.update(format!("{modified:?}").as_bytes());
    Ok(())
}

fn io_error(error: std::io::Error) -> DigestServiceError {
    DigestServiceError::Hash(crate::hash_service::EvidenceHashError::Io {
        operation: "inspect evidence revision",
        kind: error.kind(),
    })
}

// A directory mtime does not change when an existing child's bytes change.
// Walk deterministically without following symlinks and fail rather than issue
// an unverifiable revision if the bounded metadata inventory is exceeded.
fn hash_tree(hash: &mut Sha256, root: &Path) -> Result<(), DigestServiceError> {
    let mut pending = vec![root.to_path_buf()];
    let mut count = 0usize;
    while let Some(directory) = pending.pop() {
        let mut paths = std::fs::read_dir(directory)
            .map_err(io_error)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(io_error)?;
        paths.sort();
        for path in paths {
            count += 1;
            if count > 1_000_000 {
                return Err(DigestServiceError::Unsupported);
            }
            let metadata = std::fs::symlink_metadata(&path).map_err(io_error)?;
            hash.update(path.as_os_str().to_string_lossy().as_bytes());
            hash.update(metadata.len().to_le_bytes());
            hash.update(format!("{:?}", metadata.modified().map_err(io_error)?).as_bytes());
            if metadata.is_dir() {
                pending.push(path);
            }
        }
    }
    Ok(())
}
