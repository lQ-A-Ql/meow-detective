use super::super::{CephFsDescriptor, CephFsDescriptorState};
use super::CephFsJournalReplayError;

pub(super) struct ValidatedRankBinding {
    pub rank: u32,
    pub gid: u64,
    pub incarnation: i32,
}

pub(super) fn validate_rank_binding(
    descriptor: &CephFsDescriptor,
    rank: u32,
) -> Result<ValidatedRankBinding, CephFsJournalReplayError> {
    if descriptor.state != CephFsDescriptorState::Present
        || rank >= 0x100
        || descriptor.identity.trim().is_empty()
        || descriptor.fsmap_epoch == 0
        || descriptor.mdsmap_epoch == 0
        || descriptor.metadata_pool.pool_id <= 0
    {
        return Err(CephFsJournalReplayError::InvalidRankBinding { rank });
    }
    let mut bindings = descriptor
        .rank_bindings
        .iter()
        .filter(|binding| binding.rank == rank);
    let binding = bindings
        .next()
        .ok_or(CephFsJournalReplayError::InvalidRankBinding { rank })?;
    if bindings.next().is_some() || binding.incarnation < 0 {
        return Err(CephFsJournalReplayError::InvalidRankBinding { rank });
    }
    let matches_current_daemon = descriptor.daemons.iter().any(|daemon| {
        daemon.rank == rank as i32
            && daemon.gid == binding.gid
            && daemon.incarnation == binding.incarnation
            && daemon.state.is_active()
    });
    if !matches_current_daemon {
        return Err(CephFsJournalReplayError::InvalidRankBinding { rank });
    }
    Ok(ValidatedRankBinding {
        rank,
        gid: binding.gid,
        incarnation: binding.incarnation,
    })
}
