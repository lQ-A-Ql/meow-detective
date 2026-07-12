pub(crate) mod parser;
pub(crate) mod recovery;
pub(crate) mod types;

pub use parser::{collect_descriptor_blocks, parse_descriptor_block};
pub use recovery::recover_deleted_inodes;
pub use types::{
    BlockTag, DescriptorBlock, JournalHeader, JournalSuperblock, RecoveredFile, JBD2_COMMIT_MAGIC,
    JBD2_DESCRIPTOR_MAGIC, JBD2_MAGIC, JBD2_REVOKE_MAGIC, JBD2_TAG_SIZE_V2, JOURNAL_HEADER_SIZE,
    JOURNAL_INODE, JOURNAL_SB_OFFSET,
};

#[cfg(test)]
#[path = "../../tests/unit/journal.rs"]
mod tests;
