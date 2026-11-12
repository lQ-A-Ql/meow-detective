use persistence_sqlite::repositories::{
    ceph_fs_journal_repo::{CephFsJournalRepo, CephFsJournalRepoError, CephFsJournalWriteOutcome},
    ceph_fs_metadata_inventory_repo::{
        CephFsMetadataInventoryRepo, CephFsMetadataInventoryRepoError,
    },
};

use super::{
    projection::build_projection, CephFsJournalPersistenceError, CephFsJournalPersistenceOutcome,
    CephFsJournalReplay,
};
use crate::ceph_reconstruction::CephFsDescriptor;

pub fn persist_cephfs_journal_replay(
    conn: &rusqlite::Connection,
    replay: &CephFsJournalReplay,
    descriptor: &CephFsDescriptor,
    data_source_id: &str,
    inventory_id: &str,
) -> Result<CephFsJournalPersistenceOutcome, CephFsJournalPersistenceError> {
    let metadata_inventory = CephFsMetadataInventoryRepo::new(conn)
        .find(&replay.filesystem_identity, inventory_id)
        .map_err(map_metadata_error)?
        .ok_or(CephFsJournalPersistenceError::MetadataInventoryUnavailable)?;
    let projection = build_projection(
        replay,
        descriptor,
        &metadata_inventory.manifest,
        data_source_id,
        inventory_id,
    )?;
    match CephFsJournalRepo::new(conn)
        .replace(&projection)
        .map_err(map_repo_error)?
    {
        CephFsJournalWriteOutcome::Replaced => Ok(CephFsJournalPersistenceOutcome::Replaced),
        CephFsJournalWriteOutcome::Unchanged => Ok(CephFsJournalPersistenceOutcome::Unchanged),
    }
}

fn map_metadata_error(error: CephFsMetadataInventoryRepoError) -> CephFsJournalPersistenceError {
    match error {
        CephFsMetadataInventoryRepoError::Database(_) => CephFsJournalPersistenceError::Database,
        CephFsMetadataInventoryRepoError::Invalid(_)
        | CephFsMetadataInventoryRepoError::DeterminismConflict
        | CephFsMetadataInventoryRepoError::CrossPoolReference => {
            CephFsJournalPersistenceError::MetadataInventoryUnavailable
        }
    }
}

fn map_repo_error(error: CephFsJournalRepoError) -> CephFsJournalPersistenceError {
    match error {
        CephFsJournalRepoError::Invalid(_) => CephFsJournalPersistenceError::InvalidProjection,
        CephFsJournalRepoError::SourceBindingMismatch => {
            CephFsJournalPersistenceError::SourceBindingMismatch
        }
        CephFsJournalRepoError::ObjectBindingMismatch => {
            CephFsJournalPersistenceError::ObjectBindingMismatch
        }
        CephFsJournalRepoError::DeterminismConflict => {
            CephFsJournalPersistenceError::DeterminismConflict
        }
        CephFsJournalRepoError::Database(_) => CephFsJournalPersistenceError::Database,
    }
}
