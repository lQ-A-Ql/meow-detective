use domain::{CaseId, DataSourceId, DataSourceKind, DataSourcePlatform};
use persistence_sqlite::{
    repositories::datasource_repo::{DataSourceRepo, DataSourceStorage},
    DbError,
};
use rusqlite::Connection;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReadySourceError {
    #[error("database error: {0}")]
    Db(#[from] DbError),
    #[error("data source '{data_source_id}' does not belong to case '{case_id}'")]
    NotFound {
        case_id: String,
        data_source_id: String,
    },
    #[error("data source '{data_source_id}' is not ready (state: {state})")]
    NotReady {
        data_source_id: String,
        state: String,
    },
    #[error("data source '{data_source_id}' platform is unsupported: {reason}")]
    UnsupportedPlatform {
        data_source_id: String,
        reason: String,
    },
}

impl ReadySourceError {
    pub(crate) fn into_db_error(self) -> DbError {
        match self {
            Self::Db(error) => error,
            other => DbError::System(other.to_string()),
        }
    }
}

pub struct ReadySourceConnection {
    pub data_source_id: DataSourceId,
    pub platform: DataSourcePlatform,
    pub connection: Connection,
}

pub struct ReconstructionSourceConnection {
    pub data_source_id: DataSourceId,
    pub platform: DataSourcePlatform,
    pub connection: Connection,
}

pub fn open_ready_source_by_id(
    case_conn: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<ReadySourceConnection, ReadySourceError> {
    let platform = resolve_ready_source_platform(case_conn, case_id, data_source_id)?;
    let connection = super::open_registered_source_db(case_conn, case_root, data_source_id)?;

    Ok(ReadySourceConnection {
        data_source_id: data_source_id.clone(),
        platform,
        connection,
    })
}

pub fn open_ready_source_read_only_by_id(
    case_conn: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<ReadySourceConnection, ReadySourceError> {
    let platform = resolve_ready_source_platform(case_conn, case_id, data_source_id)?;
    let connection =
        super::open_registered_source_db_read_only(case_conn, case_root, data_source_id)?;

    Ok(ReadySourceConnection {
        data_source_id: data_source_id.clone(),
        platform,
        connection,
    })
}

pub fn open_reconstruction_source_by_id(
    case_conn: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<ReconstructionSourceConnection, ReadySourceError> {
    let storage = source_storage_for_case(case_conn, case_id, data_source_id)?;
    let platform = validate_reconstruction_storage(data_source_id, &storage)?;
    let connection = super::open_registered_reconstruction_source_db_read_only(
        case_conn,
        case_root,
        data_source_id,
    )?;

    Ok(ReconstructionSourceConnection {
        data_source_id: data_source_id.clone(),
        platform,
        connection,
    })
}

pub(crate) fn open_catalog_recovery_source_by_id(
    case_conn: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<Connection, ReadySourceError> {
    let repo = DataSourceRepo::new(case_conn);
    let source = repo
        .find_by_case(case_id)?
        .into_iter()
        .find(|source| source.id == *data_source_id)
        .ok_or_else(|| ReadySourceError::NotFound {
            case_id: case_id.0.clone(),
            data_source_id: data_source_id.0.clone(),
        })?;
    let storage = repo
        .find_storage(data_source_id)?
        .ok_or_else(|| ReadySourceError::NotFound {
            case_id: case_id.0.clone(),
            data_source_id: data_source_id.0.clone(),
        })?;
    validate_catalog_recovery_source(&source.kind, data_source_id, &storage)?;
    Ok(super::open_registered_source_db(
        case_conn,
        case_root,
        data_source_id,
    )?)
}

pub fn resolve_ready_source_platform(
    case_conn: &Connection,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<DataSourcePlatform, ReadySourceError> {
    let storage = source_storage_for_case(case_conn, case_id, data_source_id)?;
    validate_ready_storage(data_source_id, &storage)
}

fn source_storage_for_case(
    case_conn: &Connection,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<DataSourceStorage, ReadySourceError> {
    let repo = DataSourceRepo::new(case_conn);
    let belongs_to_case = repo
        .find_by_case(case_id)?
        .into_iter()
        .any(|source| source.id == *data_source_id);
    if !belongs_to_case {
        return Err(ReadySourceError::NotFound {
            case_id: case_id.0.clone(),
            data_source_id: data_source_id.0.clone(),
        });
    }

    repo.find_storage(data_source_id)?
        .ok_or_else(|| ReadySourceError::NotFound {
            case_id: case_id.0.clone(),
            data_source_id: data_source_id.0.clone(),
        })
}

fn validate_reconstruction_storage(
    data_source_id: &DataSourceId,
    storage: &DataSourceStorage,
) -> Result<DataSourcePlatform, ReadySourceError> {
    if !matches!(
        storage.import_state.trim().to_ascii_lowercase().as_str(),
        "ready" | "ready_metadata"
    ) {
        return Err(ReadySourceError::NotReady {
            data_source_id: data_source_id.0.clone(),
            state: storage.import_state.clone(),
        });
    }
    parse_platform(data_source_id, storage)
}

fn validate_catalog_recovery_source(
    kind: &DataSourceKind,
    data_source_id: &DataSourceId,
    storage: &DataSourceStorage,
) -> Result<(), ReadySourceError> {
    let state = storage.import_state.trim().to_ascii_lowercase();
    let platform = parse_platform(data_source_id, storage)?;
    let valid = *kind == DataSourceKind::CephRbd
        && platform == DataSourcePlatform::Linux
        && storage.profile.as_deref() == Some("vm_disk")
        && matches!(state.as_str(), "pending" | "failed" | "ready");
    if valid {
        return Ok(());
    }
    Err(ReadySourceError::UnsupportedPlatform {
        data_source_id: data_source_id.0.clone(),
        reason: format!(
            "Catalog recovery expected Ceph RBD Linux vm_disk in pending, failed, or ready state; found kind={kind}, platform={platform}, profile={}, state={}",
            storage.profile.as_deref().unwrap_or("<none>"),
            storage.import_state
        ),
    })
}

pub(super) fn validate_ready_storage(
    data_source_id: &DataSourceId,
    storage: &DataSourceStorage,
) -> Result<DataSourcePlatform, ReadySourceError> {
    if !storage.import_state.trim().eq_ignore_ascii_case("ready") {
        return Err(ReadySourceError::NotReady {
            data_source_id: data_source_id.0.clone(),
            state: storage.import_state.clone(),
        });
    }

    parse_platform(data_source_id, storage)
}

fn parse_platform(
    data_source_id: &DataSourceId,
    storage: &DataSourceStorage,
) -> Result<DataSourcePlatform, ReadySourceError> {
    DataSourcePlatform::parse_explicit(&storage.platform).map_err(|error| {
        ReadySourceError::UnsupportedPlatform {
            data_source_id: data_source_id.0.clone(),
            reason: error.to_string(),
        }
    })
}

/// Open only fully imported source databases for case-wide aggregation.
pub fn open_ready_source_connections(
    case_conn: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
) -> Result<Vec<(DataSourceId, Connection)>, ReadySourceError> {
    open_ready_source_connections_with(case_conn, case_root, case_id, open_ready_source_by_id)
}

/// Read-only variant of [`open_ready_source_connections`] for pure query
/// paths. Migrations are intentionally not run here; case opening migrates
/// ready source databases before read-only consumers see them.
pub fn open_ready_source_connections_read_only(
    case_conn: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
) -> Result<Vec<(DataSourceId, Connection)>, ReadySourceError> {
    open_ready_source_connections_with(
        case_conn,
        case_root,
        case_id,
        open_ready_source_read_only_by_id,
    )
}

fn open_ready_source_connections_with(
    case_conn: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    open: fn(
        &Connection,
        &std::path::Path,
        &CaseId,
        &DataSourceId,
    ) -> Result<ReadySourceConnection, ReadySourceError>,
) -> Result<Vec<(DataSourceId, Connection)>, ReadySourceError> {
    let sources = super::ready_data_sources(case_conn, case_id)?;
    let mut connections = Vec::with_capacity(sources.len());
    for (source, _) in sources {
        let ready = open(case_conn, case_root, case_id, &source.id)?;
        connections.push((ready.data_source_id, ready.connection));
    }
    Ok(connections)
}

#[derive(Debug, Clone)]
pub struct SourceConnectionManager {
    locator: super::SourceDbLocator,
}

impl SourceConnectionManager {
    pub fn new(case_root: impl Into<std::path::PathBuf>) -> Self {
        Self {
            locator: super::SourceDbLocator::new(case_root),
        }
    }

    pub fn open_ready(
        &self,
        case_conn: &Connection,
        case_id: &CaseId,
        data_source_id: &DataSourceId,
    ) -> Result<Connection, ReadySourceError> {
        Ok(
            open_ready_source_by_id(case_conn, &self.locator.case_root, case_id, data_source_id)?
                .connection,
        )
    }

    /// Read-only variant of [`SourceConnectionManager::open_ready`] for pure
    /// query paths such as file browsing and previews. Does not run
    /// migrations or journal-mode changes on open.
    pub fn open_ready_read_only(
        &self,
        case_conn: &Connection,
        case_id: &CaseId,
        data_source_id: &DataSourceId,
    ) -> Result<Connection, ReadySourceError> {
        Ok(open_ready_source_read_only_by_id(
            case_conn,
            &self.locator.case_root,
            case_id,
            data_source_id,
        )?
        .connection)
    }

    pub fn open_ready_for_global_file_id(
        &self,
        case_conn: &Connection,
        case_id: &CaseId,
        file_id: &domain::FileEntryId,
    ) -> Result<(super::GlobalFileId, Connection), ReadySourceError> {
        let global_id = super::GlobalFileId::parse(file_id)?;
        let conn = self.open_ready(case_conn, case_id, &global_id.data_source_id)?;
        Ok((global_id, conn))
    }

    /// Read-only variant of [`SourceConnectionManager::open_ready_for_global_file_id`].
    pub fn open_ready_for_global_file_id_read_only(
        &self,
        case_conn: &Connection,
        case_id: &CaseId,
        file_id: &domain::FileEntryId,
    ) -> Result<(super::GlobalFileId, Connection), ReadySourceError> {
        let global_id = super::GlobalFileId::parse(file_id)?;
        let conn = self.open_ready_read_only(case_conn, case_id, &global_id.data_source_id)?;
        Ok((global_id, conn))
    }
}
