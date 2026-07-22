use super::{
    RegionOrigin, XfsMetadataCandidate, XfsMetadataCandidateKind, XfsRecoveryCompleteness,
    XfsTransactionHeader, XFS_LI_ATTRD, XFS_LI_ATTRI, XFS_LI_BUD, XFS_LI_BUF, XFS_LI_BUI,
    XFS_LI_CUD, XFS_LI_CUI, XFS_LI_DQUOT, XFS_LI_EFD, XFS_LI_EFI, XFS_LI_ICREATE, XFS_LI_INODE,
    XFS_LI_IUNLINK, XFS_LI_QUOTAOFF, XFS_LI_RUD, XFS_LI_RUI, XFS_LI_XMD, XFS_LI_XMI,
};
use crate::log::inode_item::parse_inode_log_format;
use crate::log::{XfsDeletionStatus, XfsLogFormat};

const XFS_TRANS_HEADER_MAGIC: u32 = 0x5452_414E;
const XFS_BLF_CANCEL: u16 = 1 << 1;
const XFS_BLF_MAX_MAP_WORDS: u32 = 17;
const XFS_LOG_ITEM_MAX_REGIONS: u16 = 257;

pub(super) fn parse_transaction_header(
    format: XfsLogFormat,
    region: &[u8],
) -> Option<XfsTransactionHeader> {
    if region.len() != 16 {
        return None;
    }
    if format.native_u32(region, 0)? != XFS_TRANS_HEADER_MAGIC {
        return None;
    }
    Some(XfsTransactionHeader {
        transaction_type: format.native_u32(region, 4)?,
        transaction_id: format.native_u32(region, 8)? as i32,
        item_count: format.native_u32(region, 12)?,
    })
}

pub(super) fn parse_metadata_candidate(
    origin: RegionOrigin,
    region: &[u8],
) -> Option<XfsMetadataCandidate> {
    let item_type = origin.record_format.native_u16(region, 0)?;
    let region_count = origin.record_format.native_u16(region, 2)?;
    if region_count == 0 {
        return None;
    }
    let kind = metadata_kind(item_type)?;
    let mut candidate = XfsMetadataCandidate {
        transaction_id: origin.transaction_id,
        record_lsn: origin.record_lsn,
        record_log_block: origin.record_log_block,
        record_source_offset: origin.record_source_offset,
        record_checksum_status: origin.record_checksum_status,
        operation_index: origin.operation_index,
        item_type,
        kind,
        inode: None,
        disk_block: None,
        region_count,
        fields: None,
        transaction_committed: false,
        completeness: XfsRecoveryCompleteness::MetadataOnly,
        deletion_status: XfsDeletionStatus::NotProven,
    };

    match item_type {
        XFS_LI_INODE => {
            let descriptor = parse_inode_log_format(origin.record_format, region).ok()?;
            candidate.fields = Some(descriptor.fields);
            candidate.inode = Some(descriptor.inode);
            candidate.disk_block = Some(descriptor.disk_block);
        }
        XFS_LI_BUF => parse_buffer_descriptor(origin.record_format, region, &mut candidate)?,
        _ => {}
    }
    Some(candidate)
}

pub(super) fn parse_item_header(format: XfsLogFormat, region: &[u8]) -> Option<(u16, u16)> {
    let item_type = format.native_u16(region, 0)?;
    let region_count = format.native_u16(region, 2)?;
    if region_count == 0 || region_count > XFS_LOG_ITEM_MAX_REGIONS {
        return None;
    }
    Some((item_type, region_count))
}

fn parse_buffer_descriptor(
    format: XfsLogFormat,
    region: &[u8],
    candidate: &mut XfsMetadataCandidate,
) -> Option<()> {
    let flags = format.native_u16(region, 4)?;
    let map_words = format.native_u32(region, 16)?;
    if map_words == 0 || map_words > XFS_BLF_MAX_MAP_WORDS {
        return None;
    }
    let descriptor_len = 20usize.checked_add(usize::try_from(map_words).ok()?.checked_mul(4)?)?;
    if descriptor_len > region.len() {
        return None;
    }
    candidate.disk_block = Some(format.native_i64(region, 8)?);
    if flags & XFS_BLF_CANCEL != 0 {
        candidate.kind = XfsMetadataCandidateKind::BufferCancellation;
    }
    Some(())
}

fn metadata_kind(item_type: u16) -> Option<XfsMetadataCandidateKind> {
    Some(match item_type {
        XFS_LI_EFI => XfsMetadataCandidateKind::ExtentFreeIntent,
        XFS_LI_EFD => XfsMetadataCandidateKind::ExtentFreeDone,
        XFS_LI_IUNLINK => XfsMetadataCandidateKind::UnlinkedInodeUpdate,
        XFS_LI_INODE => XfsMetadataCandidateKind::InodeUpdate,
        XFS_LI_BUF => XfsMetadataCandidateKind::BufferUpdate,
        XFS_LI_DQUOT => XfsMetadataCandidateKind::DquotUpdate,
        XFS_LI_QUOTAOFF => XfsMetadataCandidateKind::QuotaOff,
        XFS_LI_ICREATE => XfsMetadataCandidateKind::InodeCreate,
        XFS_LI_RUI => XfsMetadataCandidateKind::ReverseMapIntent,
        XFS_LI_RUD => XfsMetadataCandidateKind::ReverseMapDone,
        XFS_LI_CUI => XfsMetadataCandidateKind::RefcountIntent,
        XFS_LI_CUD => XfsMetadataCandidateKind::RefcountDone,
        XFS_LI_BUI => XfsMetadataCandidateKind::BtreeIntent,
        XFS_LI_BUD => XfsMetadataCandidateKind::BtreeDone,
        XFS_LI_ATTRI => XfsMetadataCandidateKind::AttributeIntent,
        XFS_LI_ATTRD => XfsMetadataCandidateKind::AttributeDone,
        XFS_LI_XMI => XfsMetadataCandidateKind::MappingExchangeIntent,
        XFS_LI_XMD => XfsMetadataCandidateKind::MappingExchangeDone,
        _ => return None,
    })
}
