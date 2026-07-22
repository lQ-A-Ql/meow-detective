//! Read-only parsing of the native XFS journal wire format.
//!
//! The implementation follows `xfs_log_format.h`, `xfs_log_recover.c`, and
//! xfsprogs' log-print/recovery code. It reports transaction metadata only;
//! journal regions are never presented as recovered file contents.

mod checksum;
mod error;
mod geometry;
mod inode_item;
mod operation;
mod record;
mod record_validation;
mod recovery;
mod transaction;
mod wire;

pub use error::{XfsLogError, XfsLogIssue, XfsLogIssueKind};
pub use geometry::{XfsLogGeometry, XfsLogLocation, XfsLogSnapshot, XFS_LOG_MAX_SNAPSHOT_BYTES};
pub use operation::{
    XfsLogClient, XfsLogOperation, XfsLogOperationFlags, XLOG_COMMIT_TRANS, XLOG_CONTINUE_TRANS,
    XLOG_END_TRANS, XLOG_START_TRANS, XLOG_UNMOUNT_TRANS, XLOG_WAS_CONT_TRANS,
};
pub use record::{
    LogRecordHeader, XfsLogChecksumStatus, XfsLogRecord, XfsLogRecordProvenance, XfsLogSourceSpan,
};
pub use recovery::{analyze_log_snapshot, XfsLogAnalysis, XfsLogParseLimits, XfsParsedLogRecord};
pub use transaction::{
    XfsDeletedFileCandidate, XfsDeletionProof, XfsDeletionStatus, XfsLogTransaction,
    XfsMetadataCandidate, XfsMetadataCandidateKind, XfsRecoveryCompleteness, XfsTransactionHeader,
    XFS_LI_ATTRD, XFS_LI_ATTRI, XFS_LI_BUD, XFS_LI_BUF, XFS_LI_BUI, XFS_LI_CUD, XFS_LI_CUI,
    XFS_LI_DQUOT, XFS_LI_EFD, XFS_LI_EFI, XFS_LI_ICREATE, XFS_LI_INODE, XFS_LI_IUNLINK,
    XFS_LI_QUOTAOFF, XFS_LI_RUD, XFS_LI_RUI, XFS_LI_XMD, XFS_LI_XMI,
};
pub use wire::XfsLogFormat;

pub const XLOG_HEADER_MAGIC_NUM: u32 = 0xFEED_BABE;
pub const XLOG_BASIC_BLOCK_SIZE: usize = 512;
pub const XLOG_HEADER_CYCLE_SIZE: usize = 32 * 1024;
pub const XLOG_MIN_RECORD_BSIZE: usize = 16 * 1024;
pub const XLOG_BIG_RECORD_BSIZE: usize = 32 * 1024;
pub const XLOG_MAX_RECORD_BSIZE: usize = 256 * 1024;
pub const XLOG_OP_HEADER_SIZE: usize = 12;
pub const XFS_TRANSACTION_CLIENT: u8 = 0x69;
pub const XFS_LOG_CLIENT: u8 = 0xAA;

#[cfg(test)]
#[path = "../../tests/unit/log.rs"]
mod tests;
