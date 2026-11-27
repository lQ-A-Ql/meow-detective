use std::path::Path;

use domain::DataSourceId;
use persistence_sqlite::repositories::{
    catalog_publication_repo::{seal_for, CatalogPublicationRepo},
    datasource_repo::DataSourceRepo,
    processing_phase_repo::{
        DataSourceProcessingPhaseRecord, DataSourceProcessingPhaseRepo, ProcessingPhase,
    },
    source_revision_repo::SourceRevisionRepo,
    structure_view_attempt_repo::StructureViewAttemptRepo,
};

use super::{StructurePublicationError, STRUCTURE_VIEW_GENERATION};
use crate::{
    control::paths::{canonical_source_db_rel_path, source_db_path},
    publication::build::publish_source_build_db,
};

pub fn restore_prepared_build(
    conn: &rusqlite::Connection,
    case_root: &Path,
    source_id: &DataSourceId,
    claim: &DataSourceProcessingPhaseRecord,
) -> Result<(), StructurePublicationError> {
    let transaction = conn.unchecked_transaction()?;
    if claim.data_source_id != *source_id || claim.phase != ProcessingPhase::Catalog {
        return Err(StructurePublicationError::InvalidPublication);
    }
    DataSourceProcessingPhaseRepo::new(&transaction).heartbeat(
        source_id,
        ProcessingPhase::Catalog,
        claim.version,
        &claim.input_fingerprint,
        claim
            .owner_id
            .as_deref()
            .ok_or(StructurePublicationError::InvalidPublication)?,
        claim
            .attempt_id
            .as_deref()
            .ok_or(StructurePublicationError::InvalidPublication)?,
    )?;
    let publication = CatalogPublicationRepo::new(&transaction)
        .find(source_id)?
        .ok_or(StructurePublicationError::InvalidPublication)?;
    let case_id = DataSourceRepo::new(&transaction).case_id(source_id)?;
    SourceRevisionRepo::new(&transaction).require_current_inputs(&case_id.0, &source_id.0)?;
    let view = StructureViewAttemptRepo::new(&transaction)
        .find_for_attempt(&case_id.0, &source_id.0, &publication.attempt_id)?
        .ok_or(StructurePublicationError::InvalidPublication)?;
    let path = canonical_source_db_rel_path(source_id);
    if publication.state != "prepared"
        || publication.source_db_rel_path != path
        || publication.seal
            != seal_for(
                &source_id.0,
                &publication.attempt_id,
                &publication.input_fingerprint,
                &path,
                &publication.catalog_digest,
            )
        || view.input_fingerprint != publication.input_fingerprint
        || view.receipt != publication.catalog_digest
        || view.generation != STRUCTURE_VIEW_GENERATION
    {
        return Err(StructurePublicationError::InvalidPublication);
    }
    if !source_db_path(case_root, source_id).exists() {
        let layout = persistence_fjall::SourceBuildLayout::for_case_source(
            case_root,
            &source_id.0,
            &publication.attempt_id,
        )?;
        let generation = if layout.build_path().is_dir() {
            layout.build_path()
        } else {
            layout.published_path()
        };
        let manifest =
            persistence_fjall::read_source_generation_manifest_at_path(&generation, &source_id.0)?;
        if manifest.attempt_id != publication.attempt_id
            || manifest.projection_batch_digest != publication.input_fingerprint
            || manifest.immutable_data_digest != publication.catalog_digest
        {
            return Err(StructurePublicationError::InvalidPublication);
        }
        publish_source_build_db(case_root, source_id, &publication.attempt_id)?;
    }
    transaction.commit()?;
    Ok(())
}
