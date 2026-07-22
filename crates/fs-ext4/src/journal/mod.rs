pub(crate) mod checksum;
mod commit;
mod content;
mod content_allocation;
mod content_builder;
mod descriptor;
mod error;
mod inode_checksum;
mod recovery;
mod revoke;
mod ring;
mod snapshot;
mod types;

pub use commit::parse_commit_block;
pub use content::{
    DeletedContentMapping, DeletedContentMappingState, DeletedContentRange,
    DeletedContentRangeKind, RecoveryAllocationState,
};
pub use descriptor::parse_descriptor_block;
pub use error::{JournalError, JournalResult};
pub use recovery::{
    recover_deleted_inodes, DeletedInodeCandidate, DeletedInodeKind, RecoveryCompleteness,
};
pub use revoke::parse_revoke_block;
pub use ring::{parse_journal, parse_journal_history};
pub use types::{
    BlockTag, CommitBlock, DescriptorBlock, IncompleteTransaction, JournalBlockMapping,
    JournalBlockType, JournalCommit, JournalDescriptor, JournalHeader, JournalHistoryScan,
    JournalRevoke, JournalScan, JournalScanIssue, JournalSuperblock, JournalSuperblockVersion,
    JournalTagChecksum, JournalTagFormat, JournalTransaction, RevokeBlock,
    JBD2_FEATURE_COMPAT_CHECKSUM, JBD2_FEATURE_INCOMPAT_64BIT, JBD2_FEATURE_INCOMPAT_ASYNC_COMMIT,
    JBD2_FEATURE_INCOMPAT_CSUM_V2, JBD2_FEATURE_INCOMPAT_CSUM_V3,
    JBD2_FEATURE_INCOMPAT_FAST_COMMIT, JBD2_FEATURE_INCOMPAT_REVOKE, JBD2_FLAG_DELETED,
    JBD2_FLAG_ESCAPE, JBD2_FLAG_LAST_TAG, JBD2_FLAG_SAME_UUID, JBD2_MAGIC_NUMBER,
    JOURNAL_HEADER_SIZE, JOURNAL_INODE, JOURNAL_SUPERBLOCK_SIZE,
};

#[cfg(test)]
#[path = "../../tests/unit/journal.rs"]
mod tests;
