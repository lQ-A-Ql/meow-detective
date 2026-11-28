use std::collections::BTreeSet;

use domain::structure::{
    canonical_identity, ByteRange, CapabilityIntegrity, CapabilityKind, CapabilityRef,
    SourceCaptureRef, TargetFailure, TargetSelector,
};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, source_revision_repo::SourceRevisionRepo,
};

use super::super::{frontier::CapabilityEntry, RuntimeError};
use super::definition::requested_coverage;

pub fn registered_source_seed(
    conn: &rusqlite::Connection,
    capture: &SourceCaptureRef,
    scope_id: &str,
) -> Result<CapabilityEntry, RuntimeError> {
    capture
        .validate()
        .map_err(|_| RuntimeError::InvalidDefinition)?;
    let repository = SourceRevisionRepo::new(conn);
    repository.require_current(&capture.source)?;
    let (_, content) = repository.current(&capture.source.case_id, &capture.source.source_id)?;
    if content.acquisition_id != capture.acquisition_id || content.logical_size == 0 {
        return Err(RuntimeError::InvalidDefinition);
    }
    let kind = DataSourceRepo::new(conn)
        .source_kind(&domain::DataSourceId(capture.source.source_id.clone()))?;
    if !matches!(
        kind,
        domain::DataSourceKind::Raw | domain::DataSourceKind::E01
    ) {
        return Err(RuntimeError::Target(TargetFailure::UnsupportedFeature {
            capability: CapabilityKind::AcquiredBytes,
        }));
    }
    let view_id = canonical_identity("registered-source-view-v1", capture)?;
    let target = TargetSelector::Block {
        view_id: view_id.clone(),
        range: ByteRange::new(0, content.logical_size)
            .map_err(|_| RuntimeError::InvalidDefinition)?,
    };
    Ok(CapabilityEntry {
        capability: CapabilityRef::new(
            target
                .identity()
                .map_err(|_| RuntimeError::InvalidDefinition)?,
            view_id,
            CapabilityKind::AcquiredBytes,
            capture.source.revision,
            CapabilityIntegrity::Observed,
            requested_coverage(
                content.logical_size,
                match target {
                    TargetSelector::Block { range, .. } => range,
                    _ => unreachable!(),
                },
            ),
        )
        .map_err(|_| RuntimeError::InvalidOutput)?,
        scope_id: scope_id.into(),
        native_roles: BTreeSet::from(["acquired-source".into()]),
    })
}
