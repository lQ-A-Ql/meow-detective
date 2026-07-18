use domain::{CaseId, DataSourceId, DataSourcePlatform};
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

pub fn open_reconstruction_source_by_id(
    case_conn: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<ReconstructionSourceConnection, ReadySourceError> {
    let storage = source_storage_for_case(case_conn, case_id, data_source_id)?;
    let platform = validate_reconstruction_storage(data_source_id, &storage)?;
    let connection =
        super::open_registered_source_db_read_only(case_conn, case_root, data_source_id)?;

    Ok(ReconstructionSourceConnection {
        data_source_id: data_source_id.clone(),
        platform,
        connection,
    })
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
