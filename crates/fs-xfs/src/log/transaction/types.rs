use crate::log::{XfsLogChecksumStatus, XfsLogClient, XfsLogSourceSpan};

pub const XFS_LI_EFI: u16 = 0x1236;
pub const XFS_LI_EFD: u16 = 0x1237;
pub const XFS_LI_IUNLINK: u16 = 0x1238;
pub const XFS_LI_INODE: u16 = 0x123B;
pub const XFS_LI_BUF: u16 = 0x123C;
pub const XFS_LI_DQUOT: u16 = 0x123D;
pub const XFS_LI_QUOTAOFF: u16 = 0x123E;
pub const XFS_LI_ICREATE: u16 = 0x123F;
pub const XFS_LI_RUI: u16 = 0x1240;
pub const XFS_LI_RUD: u16 = 0x1241;
pub const XFS_LI_CUI: u16 = 0x1242;
pub const XFS_LI_CUD: u16 = 0x1243;
pub const XFS_LI_BUI: u16 = 0x1244;
pub const XFS_LI_BUD: u16 = 0x1245;
pub const XFS_LI_ATTRI: u16 = 0x1246;
pub const XFS_LI_ATTRD: u16 = 0x1247;
pub const XFS_LI_XMI: u16 = 0x1248;
pub const XFS_LI_XMD: u16 = 0x1249;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XfsTransactionHeader {
    pub transaction_type: u32,
    pub transaction_id: i32,
    /// Raw `xfs_trans_header.th_num_items` value.
    ///
    /// Modern CIL checkpoint transactions store the number of item regions
    /// (iovecs), not the number of logical log items, in this field.
    pub item_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XfsMetadataCandidateKind {
    InodeUpdate,
    BufferUpdate,
    BufferCancellation,
    ExtentFreeIntent,
    ExtentFreeDone,
    UnlinkedInodeUpdate,
    DquotUpdate,
    QuotaOff,
    InodeCreate,
    ReverseMapIntent,
    ReverseMapDone,
    RefcountIntent,
    RefcountDone,
    BtreeIntent,
    BtreeDone,
    AttributeIntent,
    AttributeDone,
    MappingExchangeIntent,
    MappingExchangeDone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XfsRecoveryCompleteness {
    MetadataOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XfsDeletionStatus {
    /// The log region is a metadata update and does not prove deletion.
    NotProven,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XfsDeletionProof {
    InodeCoreNlinkZero,
}

/// A structurally verified log-item descriptor, never a deleted-file result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XfsMetadataCandidate {
    pub transaction_id: u32,
    pub record_lsn: u64,
    pub record_log_block: u32,
    pub record_source_offset: u64,
    pub record_checksum_status: XfsLogChecksumStatus,
    pub operation_index: u32,
    pub item_type: u16,
    pub kind: XfsMetadataCandidateKind,
    pub inode: Option<u64>,
    pub disk_block: Option<i64>,
    pub region_count: u16,
    pub fields: Option<u32>,
    pub transaction_committed: bool,
    pub completeness: XfsRecoveryCompleteness,
    pub deletion_status: XfsDeletionStatus,
}

/// A deletion result backed by an explicit on-disk proof.
///
/// The candidate proves metadata deletion only. Logged payload regions are not
/// exposed as recovered file contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XfsDeletedFileCandidate {
    pub inode: u64,
    pub record_lsn: u64,
    pub record_log_block: u32,
    pub record_source_offset: u64,
    pub operation_index: u32,
    /// Complete raw log-record spans covering the inode descriptor and core.
    pub provenance: Vec<XfsLogSourceSpan>,
    pub proof: XfsDeletionProof,
    pub completeness: XfsRecoveryCompleteness,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XfsLogTransaction {
    pub transaction_id: u32,
    pub client: XfsLogClient,
    pub first_lsn: u64,
    pub last_lsn: u64,
    pub started: bool,
    pub committed: bool,
    pub operation_count: u32,
    /// Complete regions including the transaction header.
    pub region_count: u32,
    /// Complete item regions excluding the transaction header.
    pub item_region_count: u32,
    pub header: Option<XfsTransactionHeader>,
}
