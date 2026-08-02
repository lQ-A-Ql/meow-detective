use domain::DataSourceId;
use persistence_sqlite::{
    repositories::processing_phase_repo::{DataSourceProcessingPhaseRepo, ProcessingPhase},
    DbError,
};

use super::fingerprint::{
    load_catalog_identity, phase_dependency_identity, phase_input_fingerprint,
    PROCESSING_PHASE_VERSION,
};

pub(in crate::derived_source_service) fn queue_post_catalog_phases(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    lineage_fingerprint: &str,
) -> Result<(), DbError> {
    let catalog_identity = load_catalog_identity(case_conn, data_source_id, lineage_fingerprint)?;
    let repository = DataSourceProcessingPhaseRepo::new(case_conn);
    for phase in ProcessingPhase::ALL.into_iter().skip(1) {
        let seed =
            phase_dependency_identity("post-catalog-pending", &[&catalog_identity, phase.as_str()]);
        let input_fingerprint = phase_input_fingerprint(&seed, phase);
        repository.upsert(
            data_source_id,
            phase,
            PROCESSING_PHASE_VERSION,
            &input_fingerprint,
        )?;
    }
    Ok(())
}
