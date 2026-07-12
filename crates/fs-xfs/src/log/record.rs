use super::{
    XfsLogEntry, XLOG_HEADER_MAGIC, XLOG_ITEM_BUF, XLOG_ITEM_BUF_CANCEL, XLOG_ITEM_EFD,
    XLOG_ITEM_EFI, XLOG_ITEM_INODE, XLOG_ITEM_QUOTAOFF, XLOG_REC_HEADER_SIZE,
};
use std::io;

pub(super) mod lh_off {
    pub const MAGIC: usize = 0;
    pub const CYCLE: usize = 2;
    pub const VERSION: usize = 4;
    pub const LEN: usize = 6;
}

#[derive(Debug, Clone)]
pub struct LogRecordHeader {
    pub magic: u16,
    pub cycle: u16,
    pub version: u16,
    pub len: u16,
}

impl LogRecordHeader {
    pub fn parse(data: &[u8]) -> io::Result<Self> {
        if data.len() < XLOG_REC_HEADER_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "log record header too short",
            ));
        }
        let magic = be_u16(data, lh_off::MAGIC);
        if magic != XLOG_HEADER_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "invalid log record magic 0x{magic:04X}, expected 0x{XLOG_HEADER_MAGIC:04X}"
                ),
            ));
        }
        Ok(Self {
            magic,
            cycle: be_u16(data, lh_off::CYCLE),
            version: be_u16(data, lh_off::VERSION),
            len: be_u16(data, lh_off::LEN),
        })
    }

    pub fn record_len(&self, block_size: u64) -> u64 {
        if self.len == 0 {
            block_size
        } else {
            u64::from(self.len)
        }
    }
}

pub fn collect_log_records(
    log_data: &[u8],
    block_size: u64,
) -> io::Result<Vec<(LogRecordHeader, Vec<u8>)>> {
    let mut records = Vec::new();
    let mut offset = 0usize;
    while offset + XLOG_REC_HEADER_SIZE <= log_data.len() {
        if be_u16(log_data, offset) == 0 {
            offset += block_size as usize;
            continue;
        }
        let header = LogRecordHeader::parse(&log_data[offset..])?;
        let record_len = header.record_len(block_size) as usize;
        let end = (offset + record_len).min(log_data.len());
        let payload = log_data[offset + XLOG_REC_HEADER_SIZE..end].to_vec();
        if payload.is_empty() {
            offset += block_size as usize;
            continue;
        }
        records.push((header, payload));
        offset = ((offset + record_len) as u64).next_multiple_of(block_size) as usize;
    }
    Ok(records)
}

pub fn parse_log_entries(payload: &[u8]) -> io::Result<Vec<XfsLogEntry>> {
    let mut entries = Vec::new();
    let mut offset = 0usize;
    while offset + 4 <= payload.len() {
        let item_type = be_u16(payload, offset);
        let item_len = usize::from(be_u16(payload, offset + 2));
        if item_len < 4 || offset + item_len > payload.len() {
            offset += 2;
            continue;
        }

        let data = payload[offset + 4..offset + item_len].to_vec();
        let (operation, target_ino) = match item_type {
            XLOG_ITEM_INODE => (
                "inode_update".to_string(),
                if data.len() >= 8 { be_u64(&data, 0) } else { 0 },
            ),
            XLOG_ITEM_BUF => ("buffer_write".to_string(), 0),
            XLOG_ITEM_EFI => ("extent_free_intent".to_string(), 0),
            XLOG_ITEM_EFD => ("extent_free_done".to_string(), 0),
            XLOG_ITEM_QUOTAOFF => ("quota_off".to_string(), 0),
            XLOG_ITEM_BUF_CANCEL => ("buffer_cancel".to_string(), 0),
            _ => {
                offset += 2;
                continue;
            }
        };
        entries.push(XfsLogEntry {
            operation,
            target_ino,
            timestamp: 0,
            data,
            item_type,
        });
        offset += item_len;
    }
    Ok(entries)
}

pub(super) fn be_u16(buf: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([buf[offset], buf[offset + 1]])
}

pub(super) fn be_u32(buf: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
}

pub(super) fn be_u64(buf: &[u8], offset: usize) -> u64 {
    u64::from_be_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
        buf[offset + 4],
        buf[offset + 5],
        buf[offset + 6],
        buf[offset + 7],
    ])
}
