use std::collections::BTreeSet;

use crate::ceph_reconstruction::{
    CephFsDescriptor, CephFsDescriptorState, CephFsPoolRole, CephFsPresenceAssessment,
    CephFsPresenceState,
};

use super::{CephFsSourceError, CephFsSourceResult};

pub(super) fn validate_presence(
    assessment: &CephFsPresenceAssessment,
    descriptor: &CephFsDescriptor,
) -> CephFsSourceResult<()> {
    if assessment.state != CephFsPresenceState::Present {
        return Err(CephFsSourceError::PresenceNotProven(
            "FSMap/MDSMap presence is not proven",
        ));
    }
    if assessment.source_count == 0 || assessment.filesystem_count != 1 {
        return Err(CephFsSourceError::PresenceNotProven(
            "the reconstruction requires exactly one proven filesystem and at least one source",
        ));
    }
    if assessment.source_ids.len() != assessment.source_count
        || assessment
            .source_ids
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != assessment.source_ids.len()
        || assessment.filesystems.len() != assessment.filesystem_count
        || assessment.cluster_identity.as_deref() != Some(descriptor.cluster_identity.as_str())
    {
        return Err(CephFsSourceError::PresenceNotProven(
            "presence source set or cluster identity does not match the filesystem descriptor",
        ));
    }
    let assessment_sources = assessment
        .source_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let descriptor_sources = descriptor
        .provenance
        .iter()
        .map(|item| item.source_identity.as_str())
        .collect::<BTreeSet<_>>();
    if descriptor_sources.is_empty() || descriptor_sources != assessment_sources {
        return Err(CephFsSourceError::PresenceNotProven(
            "presence source set does not match descriptor map provenance",
        ));
    }
    let filesystem_id = u64::try_from(descriptor.filesystem_id).map_err(|_| {
        CephFsSourceError::PresenceNotProven("filesystem identity is outside the proven range")
    })?;
    let filesystem = assessment
        .filesystems
        .first()
        .ok_or(CephFsSourceError::PresenceNotProven(
            "presence filesystem binding is missing",
        ))?;
    let metadata_pool_id = u64::try_from(descriptor.metadata_pool.pool_id).map_err(|_| {
        CephFsSourceError::PresenceNotProven("metadata pool identity is outside the proven range")
    })?;
    let mut data_pool_ids = descriptor
        .data_pools
        .iter()
        .map(|pool| match pool.role {
            CephFsPoolRole::Data { .. } => u64::try_from(pool.pool_id).map_err(|_| {
                CephFsSourceError::PresenceNotProven(
                    "data pool identity is outside the proven range",
                )
            }),
            CephFsPoolRole::Metadata => Err(CephFsSourceError::PresenceNotProven(
                "data pool list contains a metadata binding",
            )),
        })
        .collect::<CephFsSourceResult<Vec<_>>>()?;
    data_pool_ids.sort_unstable();
    if filesystem.filesystem_id != filesystem_id
        || filesystem.metadata_pool_id != metadata_pool_id
        || filesystem.data_pool_ids != data_pool_ids
    {
        return Err(CephFsSourceError::PresenceNotProven(
            "presence filesystem or pool binding does not match the descriptor",
        ));
    }
    if !assessment.diagnostics.is_empty() {
        return Err(CephFsSourceError::PresenceNotProven(
            "presence assessment contains diagnostics",
        ));
    }
    if assessment.fsmap_epoch != Some(u64::from(descriptor.fsmap_epoch))
        || assessment.mdsmap_epoch != Some(u64::from(descriptor.mdsmap_epoch))
    {
        return Err(CephFsSourceError::PresenceNotProven(
            "presence epochs do not match the filesystem descriptor",
        ));
    }
    if descriptor.state != CephFsDescriptorState::Present {
        return Err(CephFsSourceError::PresenceNotProven(
            "filesystem descriptor is not replayable",
        ));
    }
    Ok(())
}
