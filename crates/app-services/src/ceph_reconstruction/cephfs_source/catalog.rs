use domain::DataSourceId;
use persistence_sqlite::repositories::processing_phase_repo::{
    DataSourceProcessingPhaseRepo, ProcessingPhase, ProcessingPhaseClaim,
    ProcessingPhaseCompletion, ProcessingPhaseTransition,
};
use sha2::{Digest, Sha256};

use super::{CephFsSourceError, CephFsSourceResult, MaterializedCephFsSource};

const CEPHFS_CATALOG_PHASE_VERSION: u32 = 1;

pub(super) struct CatalogAttempt {
    pub owner_id: String,
    pub attempt_id: String,
    pub input_fingerprint: String,
}

pub(super) enum CatalogClaim {
    Acquired(CatalogAttempt),
    Ready,
}

pub(super) fn catalog_input_fingerprint(lineage_fingerprint: &str) -> String {
    let mut hasher = Sha256::new();
    for value in [
        b"meow-detective-cephfs-catalog-v1".as_slice(),
        lineage_fingerprint.as_bytes(),
        persistence_sqlite::migrations::runner::latest_source_version().as_bytes(),
    ] {
        hasher.update((value.len() as u64).to_le_bytes());
        hasher.update(value);
    }
    hex::encode(hasher.finalize())
}

pub(super) fn claim_catalog(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    input_fingerprint: &str,
) -> CephFsSourceResult<CatalogClaim> {
    let owner_id = format!("cephfs:{}:{}", std::process::id(), uuid::Uuid::new_v4());
    match DataSourceProcessingPhaseRepo::new(case_conn).claim(
        data_source_id,
        ProcessingPhase::Catalog,
        CEPHFS_CATALOG_PHASE_VERSION,
        input_fingerprint,
        &owner_id,
    )? {
        ProcessingPhaseClaim::Acquired(record) => Ok(CatalogClaim::Acquired(CatalogAttempt {
            owner_id,
            attempt_id: record.attempt_id.ok_or_else(|| {
                CephFsSourceError::InconsistentState(
                    "claimed Catalog phase has no attempt ID".to_string(),
                )
            })?,
            input_fingerprint: input_fingerprint.to_string(),
        })),
        ProcessingPhaseClaim::Ready(_) => Ok(CatalogClaim::Ready),
        ProcessingPhaseClaim::Busy(_) => Err(CephFsSourceError::ProcessingBusy),
    }
}

pub(super) fn refresh_catalog(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    attempt: &CatalogAttempt,
) -> CephFsSourceResult<()> {
    DataSourceProcessingPhaseRepo::new(case_conn).heartbeat(
        data_source_id,
        ProcessingPhase::Catalog,
        CEPHFS_CATALOG_PHASE_VERSION,
        &attempt.input_fingerprint,
        &attempt.owner_id,
        &attempt.attempt_id,
    )?;
    Ok(())
}

pub(super) fn complete_catalog(
    conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    attempt: &CatalogAttempt,
    summary: &MaterializedCephFsSource,
) -> CephFsSourceResult<()> {
    let stats = serde_json::json!({
        "materializerVersion": 1,
        "recordCount": summary.file_count,
        "directoryCount": summary.directory_count,
        "totalSize": summary.total_size,
        "catalogDigest": summary.catalog_digest,
        "published": true,
    })
    .to_string();
    DataSourceProcessingPhaseRepo::new(conn).finish(
        data_source_id,
        ProcessingPhase::Catalog,
        ProcessingPhaseCompletion::new(
            CEPHFS_CATALOG_PHASE_VERSION,
            &attempt.input_fingerprint,
            &attempt.owner_id,
            &attempt.attempt_id,
            ProcessingPhaseTransition::ready(&stats),
        ),
    )?;
    Ok(())
}

pub(super) fn defer_incomplete_catalog(
    conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    attempt: &CatalogAttempt,
    summary: &MaterializedCephFsSource,
) -> CephFsSourceResult<()> {
    let stats = serde_json::json!({
        "materializerVersion": 1,
        "recordCount": 0,
        "diagnosticCatalogDigest": summary.catalog_digest,
        "published": false,
    })
    .to_string();
    DataSourceProcessingPhaseRepo::new(conn).finish(
        data_source_id,
        ProcessingPhase::Catalog,
        ProcessingPhaseCompletion::new(
            CEPHFS_CATALOG_PHASE_VERSION,
            &attempt.input_fingerprint,
            &attempt.owner_id,
            &attempt.attempt_id,
            ProcessingPhaseTransition::deferred(
                &stats,
                Some("CephFS namespace closure could not be proven"),
            ),
        ),
    )?;
    Ok(())
}

pub(super) fn fail_catalog(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    attempt: &CatalogAttempt,
    error: &CephFsSourceError,
) {
    let message = error.to_string();
    if let Err(state_error) = DataSourceProcessingPhaseRepo::new(case_conn).finish(
        data_source_id,
        ProcessingPhase::Catalog,
        ProcessingPhaseCompletion::new(
            CEPHFS_CATALOG_PHASE_VERSION,
            &attempt.input_fingerprint,
            &attempt.owner_id,
            &attempt.attempt_id,
            ProcessingPhaseTransition::failed("{}", &message),
        ),
    ) {
        tracing::warn!(
            data_source_id = %data_source_id.0,
            error = %state_error,
            primary_error = %message,
            "Failed to persist CephFS Catalog failure"
        );
    }
}
