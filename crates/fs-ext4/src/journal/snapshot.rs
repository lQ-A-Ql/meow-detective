use super::error::{JournalError, JournalResult};
use super::recovery::{recover_deleted_inodes, DeletedInodeCandidate};
use super::ring::{parse_journal, parse_journal_history};
use super::types::{JournalHistoryScan, JournalScan};
use crate::Ext4Reader;

const EXT4_S_IFMT: u16 = 0xF000;
const EXT4_S_IFREG: u16 = 0x8000;

impl Ext4Reader {
    /// Reads the complete internal journal or returns a typed limit error.
    /// No prefix/truncated snapshot is returned when `max_bytes` is exceeded.
    pub fn read_internal_journal(&self, max_bytes: usize) -> JournalResult<Vec<u8>> {
        if max_bytes == 0 {
            return Err(JournalError::Invalid(
                "internal journal read limit must be non-zero".into(),
            ));
        }
        let journal_inode = match (self.has_journal, self.journal_inode) {
            (false, _) => {
                return Err(JournalError::Unsupported(
                    "filesystem does not declare a journal".into(),
                ))
            }
            (true, None) => {
                return Err(JournalError::Unsupported(
                    "external journal devices are not supported".into(),
                ))
            }
            (true, Some(inode)) => inode,
        };
        let inode = self.read_inode(journal_inode)?;
        let mode = Self::inode_mode(&inode)?;
        if mode & EXT4_S_IFMT != EXT4_S_IFREG {
            return Err(JournalError::Invalid(format!(
                "journal inode {journal_inode} is not a regular file"
            )));
        }
        let file_size = Self::inode_size(&inode)?;
        let length = usize::try_from(file_size)
            .map_err(|_| JournalError::Unsupported("journal exceeds addressable memory".into()))?;
        if length > max_bytes {
            return Err(JournalError::Unsupported(format!(
                "journal size {length} exceeds configured read limit {max_bytes}"
            )));
        }
        let data =
            self.read_extent_data_range(Self::inode_i_block(&inode), file_size, 0, length)?;
        if data.len() != length {
            return Err(JournalError::Invalid(format!(
                "journal inode declared {length} bytes but yielded {}",
                data.len()
            )));
        }
        Ok(data)
    }

    pub fn scan_internal_journal(&self, max_bytes: usize) -> JournalResult<JournalScan> {
        let journal = self.read_internal_journal(max_bytes)?;
        parse_journal(&journal)
    }

    pub fn scan_internal_journal_history(
        &self,
        max_bytes: usize,
    ) -> JournalResult<JournalHistoryScan> {
        let journal = self.read_internal_journal(max_bytes)?;
        parse_journal_history(&journal)
    }

    pub fn recover_deleted_inode_candidates(
        &self,
        max_journal_bytes: usize,
    ) -> JournalResult<Vec<DeletedInodeCandidate>> {
        let journal = self.read_internal_journal(max_journal_bytes)?;
        recover_deleted_inodes(self, &journal)
    }
}
