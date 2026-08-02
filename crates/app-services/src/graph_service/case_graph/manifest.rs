use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::case_graph_repo::CaseGraphSourceState;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::source_db;

use super::super::GraphServiceError;
use super::CASE_GRAPH_PROJECTION_VERSION;

#[derive(Debug, Clone)]
pub(super) struct SourceProjectionInput {
    pub data_source_id: DataSourceId,
    pub database_path: PathBuf,
    pub state: CaseGraphSourceState,
}

#[derive(Debug)]
pub(super) struct CaseGraphManifest {
    pub digest: String,
    pub sources: Vec<SourceProjectionInput>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestPayload<'a> {
    case_id: &'a str,
    projection_version: &'a str,
    sources: Vec<ManifestSource<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestSource<'a> {
    data_source_id: &'a str,
    schema_version: &'a str,
    database_size_bytes: u64,
    database_modified_ns: &'a str,
    wal_size_bytes: u64,
    wal_modified_ns: &'a str,
}

pub(super) fn collect_case_graph_manifest(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &str,
) -> Result<CaseGraphManifest, GraphServiceError> {
    let mut sources = Vec::new();
    for (source, storage) in source_db::ready_data_sources(case_conn, &CaseId(case_id.to_string()))?
    {
        let database_path = source_db::registered_source_db_path(case_conn, case_root, &source.id)?;
        let schema_version = storage.schema_version.ok_or_else(|| {
            GraphServiceError::InvalidInput(format!(
                "Data source '{}' has no source database schema version",
                source.id.0
            ))
        })?;
        let database = file_stamp(&database_path)?;
        let wal = file_stamp(&wal_path(&database_path))?;
        sources.push(SourceProjectionInput {
            state: CaseGraphSourceState {
                data_source_id: source.id.0.clone(),
                schema_version,
                database_size_bytes: database.size_bytes,
                database_modified_ns: database.modified_ns,
                wal_size_bytes: wal.size_bytes,
                wal_modified_ns: wal.modified_ns,
            },
            data_source_id: source.id,
            database_path,
        });
    }
    sources.sort_by(|left, right| left.data_source_id.0.cmp(&right.data_source_id.0));
    let payload = ManifestPayload {
        case_id,
        projection_version: CASE_GRAPH_PROJECTION_VERSION,
        sources: sources
            .iter()
            .map(|source| ManifestSource {
                data_source_id: &source.state.data_source_id,
                schema_version: &source.state.schema_version,
                database_size_bytes: source.state.database_size_bytes,
                database_modified_ns: &source.state.database_modified_ns,
                wal_size_bytes: source.state.wal_size_bytes,
                wal_modified_ns: &source.state.wal_modified_ns,
            })
            .collect(),
    };
    let bytes = serde_json::to_vec(&payload)?;
    Ok(CaseGraphManifest {
        digest: hex::encode(Sha256::digest(bytes)),
        sources,
    })
}

struct FileStamp {
    size_bytes: u64,
    modified_ns: String,
}

fn file_stamp(path: &Path) -> Result<FileStamp, GraphServiceError> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(FileStamp {
                size_bytes: 0,
                modified_ns: "0".to_string(),
            });
        }
        Err(error) => return Err(persistence_sqlite::DbError::Io(error).into()),
    };
    let modified_ns = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos().to_string())
        .unwrap_or_else(|| "0".to_string());
    Ok(FileStamp {
        size_bytes: metadata.len(),
        modified_ns,
    })
}

fn wal_path(database_path: &Path) -> PathBuf {
    let mut path = database_path.as_os_str().to_os_string();
    path.push("-wal");
    PathBuf::from(path)
}
