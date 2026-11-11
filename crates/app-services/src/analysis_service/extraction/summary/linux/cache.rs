use crate::analysis_service::error::AnalysisServiceError;
use persistence_fjall::{encode_key, KeyPart, KeyspaceName, PersistMode};
use persistence_source::repositories::artifact_repo::ArtifactRepo;
use persistence_sqlite::repositories::analysis_scan_repo::AnalysisScanRepo;
use rusqlite::Connection;
use serde::{de::DeserializeOwned, Serialize};

const CACHE_FAMILY: &str = "linux.summary";
const CACHE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy)]
pub(super) struct LinuxSummaryRevisions {
    artifact: u64,
    analysis: u64,
}

impl LinuxSummaryRevisions {
    pub(super) fn load(conn: &Connection) -> Result<Self, AnalysisServiceError> {
        Ok(Self {
            artifact: ArtifactRepo::new(conn).cursor_revision()?,
            analysis: AnalysisScanRepo::new(conn).revision()?,
        })
    }

    pub(super) const fn artifact(self) -> u64 {
        self.artifact
    }

    pub(super) const fn analysis(self) -> u64 {
        self.analysis
    }
}

pub(super) fn read_static<T: DeserializeOwned>(
    conn: &Connection,
    revisions: LinuxSummaryRevisions,
) -> Result<Option<T>, AnalysisServiceError> {
    read(conn, static_key(conn, revisions)?)
}

pub(super) fn write_static<T: Serialize>(
    conn: &Connection,
    revisions: LinuxSummaryRevisions,
    value: &T,
) -> Result<(), AnalysisServiceError> {
    write(conn, static_key(conn, revisions)?, value)
}

pub(super) fn read_page<T: DeserializeOwned>(
    conn: &Connection,
    revisions: LinuxSummaryRevisions,
    offset: u64,
    limit: u32,
) -> Result<Option<T>, AnalysisServiceError> {
    read(conn, page_key(conn, revisions, offset, limit)?)
}

pub(super) fn write_page<T: Serialize>(
    conn: &Connection,
    revisions: LinuxSummaryRevisions,
    offset: u64,
    limit: u32,
    value: &T,
) -> Result<(), AnalysisServiceError> {
    write(conn, page_key(conn, revisions, offset, limit)?, value)
}

fn static_key(
    conn: &Connection,
    revisions: LinuxSummaryRevisions,
) -> Result<Vec<u8>, AnalysisServiceError> {
    let source_id = source_id(conn)?;
    encode_key(
        CACHE_FAMILY,
        &[
            KeyPart::U32(CACHE_SCHEMA_VERSION),
            KeyPart::Text(source_id),
            KeyPart::Text("static".into()),
            KeyPart::U64(revisions.artifact),
            KeyPart::U64(revisions.analysis),
        ],
    )
    .map_err(|error| AnalysisServiceError::Other(error.to_string()))
}

fn page_key(
    conn: &Connection,
    revisions: LinuxSummaryRevisions,
    offset: u64,
    limit: u32,
) -> Result<Vec<u8>, AnalysisServiceError> {
    let source_id = source_id(conn)?;
    encode_key(
        CACHE_FAMILY,
        &[
            KeyPart::U32(CACHE_SCHEMA_VERSION),
            KeyPart::Text(source_id),
            KeyPart::Text("page".into()),
            KeyPart::U64(revisions.artifact),
            KeyPart::U64(revisions.analysis),
            KeyPart::U64(offset),
            KeyPart::U32(limit),
        ],
    )
    .map_err(|error| AnalysisServiceError::Other(error.to_string()))
}

fn source_id(conn: &Connection) -> Result<String, AnalysisServiceError> {
    persistence_source::detail::source_id(conn)?
        .ok_or_else(|| AnalysisServiceError::InvalidInput("source identity is missing".into()))
}

fn read<T: DeserializeOwned>(
    conn: &Connection,
    key: Vec<u8>,
) -> Result<Option<T>, AnalysisServiceError> {
    let Some(store) = persistence_source::detail::open_store(conn)? else {
        return Err(AnalysisServiceError::InvalidInput(
            "source detail storage is missing".into(),
        ));
    };
    let snapshot = store.database().read_snapshot();
    let Some(value) = store
        .read_raw(&snapshot, KeyspaceName::Meta, &key)
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?
    else {
        return Ok(None);
    };
    serde_json::from_slice(&value).map(Some).map_err(|error| {
        AnalysisServiceError::Other(format!("decode Linux summary cache: {error}"))
    })
}

fn write<T: Serialize>(
    conn: &Connection,
    key: Vec<u8>,
    value: &T,
) -> Result<(), AnalysisServiceError> {
    let Some(store) = persistence_source::detail::open_store(conn)? else {
        return Err(AnalysisServiceError::InvalidInput(
            "source detail storage is missing".into(),
        ));
    };
    let value = serde_json::to_vec(value).map_err(|error| {
        AnalysisServiceError::Other(format!("encode Linux summary cache: {error}"))
    })?;
    store
        .write_raw(KeyspaceName::Meta, key, value, PersistMode::SyncData)
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    Ok(())
}
