use domain::{
    structure::{
        NativeObservation, ObservationDetail, ObservationIndex, ObservationIndexError,
        SourceRevisionRef,
    },
    CaseId, DataSourceId,
};
use persistence_sqlite::repositories::{
    source_revision_repo::SourceRevisionRepo,
    structure_observation_repo::{ObservationState, StructureObservationRepo},
};
use rusqlite::Connection;

mod lvm;

#[derive(Debug, thiserror::Error)]
pub enum StructureObservationError {
    #[error(transparent)]
    Database(#[from] persistence_sqlite::DbError),
    #[error(transparent)]
    Storage(#[from] persistence_source::fjall::FjallStorageError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Identity(#[from] ObservationIndexError),
    #[error("structure observation publication identity is invalid")]
    InvalidIdentity,
    #[error("structure observation source generation does not match its index")]
    InvalidGeneration,
}

impl From<rusqlite::Error> for StructureObservationError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error.into())
    }
}

pub struct ObservationPublication<'a> {
    pub case_id: &'a CaseId,
    pub source: &'a SourceRevisionRef,
    pub acquisition_id: &'a str,
    pub attempt_id: &'a str,
    pub generation: &'a str,
    pub receipt: &'a str,
}

pub fn publish_source_observations(
    case_conn: &Connection,
    source_conn: &Connection,
    publication: &ObservationPublication<'_>,
    observations: &[NativeObservation],
) -> Result<Vec<ObservationIndex>, StructureObservationError> {
    if publication.acquisition_id.is_empty()
        || publication.attempt_id.is_empty()
        || publication.generation.is_empty()
    {
        return Err(StructureObservationError::InvalidIdentity);
    }
    SourceRevisionRepo::new(case_conn).require_current(publication.source)?;
    let mut indexes = Vec::with_capacity(observations.len());
    let mut indexed_observations = Vec::with_capacity(observations.len());
    for observation in observations {
        let detail = ObservationDetail {
            details: observation.details.clone(),
        };
        let digest = persistence_source::detail::observation_detail_digest(&detail)?;
        let index = ObservationIndex::new(
            publication.source.clone(),
            observation,
            digest,
            publication.attempt_id.to_owned(),
            publication.generation.to_owned(),
            publication.receipt.to_owned(),
        )?;
        persistence_source::detail::write_observation_detail(source_conn, &index, &detail)?;
        indexes.push(index.clone());
        indexed_observations.push((index, observation));
    }

    let transaction = case_conn
        .is_autocommit()
        .then(|| case_conn.unchecked_transaction())
        .transpose()?;
    let conn: &Connection = match transaction.as_ref() {
        Some(transaction) => transaction,
        None => case_conn,
    };
    let repo = StructureObservationRepo::new(conn);
    for (index, observation) in &indexed_observations {
        repo.insert(&publication.case_id.0, index, ObservationState::Published)?;
        lvm::bind_observation_objects(
            conn,
            publication.case_id,
            publication.acquisition_id,
            publication.source,
            index,
            observation,
        )?;
    }
    if let Some(transaction) = transaction {
        transaction.commit()?;
    }
    Ok(indexes)
}

pub fn read_source_observations(
    case_conn: &Connection,
    case_root: &std::path::Path,
    source: &SourceRevisionRef,
) -> Result<Vec<NativeObservation>, StructureObservationError> {
    let indexes = StructureObservationRepo::new(case_conn).list_published(source)?;
    if indexes.is_empty() {
        return Ok(Vec::new());
    }
    let source_conn = source_kernel::source_db::open_registered_source_db_read_only(
        case_conn,
        case_root,
        &DataSourceId(source.source_id.clone()),
    )?;
    let store = persistence_source::detail::open_store(&source_conn)?
        .ok_or(StructureObservationError::InvalidGeneration)?;
    let manifest =
        persistence_source::fjall::read_source_generation_manifest(&store, &source.source_id)?;
    let generation = persistence_source::detail::generation_name(&source_conn)?;
    let mut observations = Vec::with_capacity(indexes.len());
    for index in indexes {
        if index.generation != generation
            || index.attempt_id != manifest.attempt_id
            || index.receipt != manifest.projection_batch_digest
        {
            return Err(StructureObservationError::InvalidGeneration);
        }
        let detail = persistence_source::detail::read_observation_detail(&source_conn, &index)?;
        observations.push(NativeObservation {
            native_identity: index.native_identity,
            version: index.version,
            location: index.location,
            parser_version: index.parser_version,
            native_checksum_verified: index.native_checksum_verified,
            details: detail.details,
        });
    }
    Ok(observations)
}
