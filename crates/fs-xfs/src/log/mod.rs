//! XFS log parsing and deleted-inode metadata recovery.

mod record;
mod recovery;

use serde::Serialize;

pub use record::{collect_log_records, parse_log_entries, LogRecordHeader};
pub use recovery::{recover_deleted_inodes, recover_metadata_operations};

pub const XLOG_HEADER_MAGIC: u16 = 0xFEED;
pub const XLOG_REC_HEADER_SIZE: usize = 32;
pub const XLOG_DEFAULT_BLOCK_SIZE: u64 = 4096;
pub const XLOG_ITEM_BUF: u16 = 0x1234;
pub const XLOG_ITEM_INODE: u16 = 0x1235;
pub const XLOG_ITEM_EFI: u16 = 0x1236;
pub const XLOG_ITEM_EFD: u16 = 0x1237;
pub const XLOG_ITEM_QUOTAOFF: u16 = 0x1238;
pub const XLOG_ITEM_BUF_CANCEL: u16 = 0x1239;

#[derive(Debug, Clone, Serialize)]
pub struct RecoveredFile {
    pub original_path: String,
    pub inode: u64,
    pub blocks: Vec<Vec<u8>>,
    pub declared_size: u64,
    pub recovery_method: String,
    pub confidence: f64,
    pub block_count: u64,
}

#[derive(Debug, Clone)]
pub struct XfsLogEntry {
    pub operation: String,
    pub target_ino: u64,
    pub timestamp: u64,
    pub data: Vec<u8>,
    pub item_type: u16,
}

#[cfg(test)]
#[path = "../../tests/unit/log.rs"]
mod tests;
