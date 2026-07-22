use super::{XfsLogError, XfsLogIssue, XfsLogIssueKind, XLOG_BASIC_BLOCK_SIZE};
use crate::reader::{be_u16, be_u32, be_u64, sb_off};
use crate::XfsReader;

pub const XFS_LOG_MAX_SNAPSHOT_BYTES: usize = 64 * 1024 * 1024;
const XFS_SB_VERSION_NUMBITS: u16 = 0x000F;
const XFS_SB_VERSION_5: u16 = 5;
const XFS_SB_VERSION_LOGV2BIT: u16 = 0x0400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XfsLogLocation {
    Internal { start_fsb: u64 },
    External,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XfsLogGeometry {
    pub location: XfsLogLocation,
    pub block_size: u32,
    pub log_blocks: u32,
    pub sector_size: u16,
    pub record_version: u32,
    pub metadata_crc: bool,
    pub fs_uuid: [u8; 16],
}

impl XfsLogGeometry {
    pub(crate) fn from_superblock(superblock: &[u8]) -> Self {
        let block_size = be_u32(superblock, sb_off::BLOCKSIZE);
        let log_start = be_u64(superblock, sb_off::LOGSTART);
        let version = be_u16(superblock, sb_off::VERSIONNUM);
        let log_sector_size = be_u16(superblock, sb_off::LOGSECTSIZE);
        let sector_size = if log_sector_size == 0 {
            be_u16(superblock, sb_off::SECTSIZE)
        } else {
            log_sector_size
        };
        let mut fs_uuid = [0u8; 16];
        fs_uuid.copy_from_slice(&superblock[sb_off::UUID..sb_off::UUID + 16]);

        Self {
            location: if log_start == 0 {
                XfsLogLocation::External
            } else {
                XfsLogLocation::Internal {
                    start_fsb: log_start,
                }
            },
            block_size,
            log_blocks: be_u32(superblock, sb_off::LOGBLOCKS),
            sector_size,
            record_version: if (version & XFS_SB_VERSION_NUMBITS) == XFS_SB_VERSION_5
                || (version & XFS_SB_VERSION_LOGV2BIT) != 0
            {
                2
            } else {
                1
            },
            metadata_crc: (version & XFS_SB_VERSION_NUMBITS) == XFS_SB_VERSION_5,
            fs_uuid,
        }
    }

    pub fn log_bytes(&self) -> Result<u64, XfsLogError> {
        u64::from(self.log_blocks)
            .checked_mul(u64::from(self.block_size))
            .ok_or_else(|| XfsLogError::InvalidGeometry("log byte length overflows".to_string()))
    }

    pub fn basic_block_count(&self) -> Result<u64, XfsLogError> {
        Ok(self.log_bytes()? / XLOG_BASIC_BLOCK_SIZE as u64)
    }

    pub(crate) fn validate(&self) -> Result<(), XfsLogError> {
        if self.block_size < XLOG_BASIC_BLOCK_SIZE as u32
            || !self.block_size.is_power_of_two()
            || !self.block_size.is_multiple_of(XLOG_BASIC_BLOCK_SIZE as u32)
        {
            return Err(XfsLogError::InvalidGeometry(format!(
                "invalid filesystem block size {} for a 512-byte XFS log",
                self.block_size
            )));
        }
        if self.log_blocks == 0 {
            return Err(XfsLogError::InvalidGeometry(
                "superblock declares zero log blocks".to_string(),
            ));
        }
        if self.sector_size < XLOG_BASIC_BLOCK_SIZE as u16
            || !self.sector_size.is_power_of_two()
            || u32::from(self.sector_size) > self.block_size
        {
            return Err(XfsLogError::InvalidGeometry(format!(
                "invalid log sector size {} for block size {}",
                self.sector_size, self.block_size
            )));
        }
        if !matches!(self.record_version, 1 | 2) {
            return Err(XfsLogError::InvalidGeometry(format!(
                "unsupported record version {}",
                self.record_version
            )));
        }
        let basic_blocks = self.basic_block_count()?;
        if basic_blocks == 0 || basic_blocks > u64::from(u32::MAX) {
            return Err(XfsLogError::InvalidGeometry(format!(
                "log basic-block count {basic_blocks} is outside XFS bounds"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct XfsLogSnapshot {
    pub geometry: XfsLogGeometry,
    pub bytes: Vec<u8>,
    pub complete: bool,
    pub byte_limit: usize,
    pub source_offset: u64,
}

impl XfsReader {
    pub fn log_geometry(&self) -> &XfsLogGeometry {
        &self.log_geometry
    }

    pub fn read_internal_log_snapshot(
        &self,
        max_bytes: usize,
    ) -> Result<XfsLogSnapshot, XfsLogError> {
        self.log_geometry.validate()?;
        let start_fsb = match self.log_geometry.location {
            XfsLogLocation::Internal { start_fsb } => start_fsb,
            XfsLogLocation::External => {
                return Err(XfsLogError::Unsupported(XfsLogIssue::new(
                    XfsLogIssueKind::ExternalLogUnsupported,
                    None,
                    "the superblock references an external log device; no external evidence reader was supplied",
                )))
            }
        };
        let effective_limit = max_bytes.min(XFS_LOG_MAX_SNAPSHOT_BYTES) / XLOG_BASIC_BLOCK_SIZE
            * XLOG_BASIC_BLOCK_SIZE;
        if effective_limit == 0 {
            return Err(XfsLogError::InvalidGeometry(format!(
                "snapshot limit must be at least {XLOG_BASIC_BLOCK_SIZE} bytes"
            )));
        }

        let start_block = self.fsblock_to_linear_block(start_fsb)?;
        let end_block = start_block
            .checked_add(u64::from(self.log_geometry.log_blocks))
            .ok_or_else(|| XfsLogError::InvalidGeometry("internal log range overflows".into()))?;
        if end_block > self.dblocks {
            return Err(XfsLogError::InvalidGeometry(format!(
                "internal log range {start_block}..{end_block} exceeds {} data blocks",
                self.dblocks
            )));
        }

        let total_bytes = self.log_geometry.log_bytes()?;
        let read_len_u64 = total_bytes.min(effective_limit as u64);
        let read_len = usize::try_from(read_len_u64).map_err(|_| {
            XfsLogError::InvalidGeometry("snapshot length exceeds addressable memory".into())
        })?;
        let physical_offset = self.block_to_offset(start_block)?;
        let bytes = self.read_bytes_at(physical_offset, read_len)?;
        Ok(XfsLogSnapshot {
            geometry: self.log_geometry.clone(),
            bytes,
            complete: read_len_u64 == total_bytes,
            byte_limit: effective_limit,
            source_offset: physical_offset,
        })
    }
}
