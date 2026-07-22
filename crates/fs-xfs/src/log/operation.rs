use super::wire::{be_u16, be_u32};
use super::{
    XfsLogChecksumStatus, XfsLogError, XfsLogFormat, XfsLogRecord, XfsLogRecordProvenance,
    XFS_LOG_CLIENT, XFS_TRANSACTION_CLIENT, XLOG_OP_HEADER_SIZE,
};

pub const XLOG_START_TRANS: u8 = 0x01;
pub const XLOG_COMMIT_TRANS: u8 = 0x02;
pub const XLOG_CONTINUE_TRANS: u8 = 0x04;
pub const XLOG_WAS_CONT_TRANS: u8 = 0x08;
pub const XLOG_END_TRANS: u8 = 0x10;
pub const XLOG_UNMOUNT_TRANS: u8 = 0x20;
const XLOG_KNOWN_FLAGS: u8 = XLOG_START_TRANS
    | XLOG_COMMIT_TRANS
    | XLOG_CONTINUE_TRANS
    | XLOG_WAS_CONT_TRANS
    | XLOG_END_TRANS
    | XLOG_UNMOUNT_TRANS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum XfsLogClient {
    Transaction,
    Log,
}

impl XfsLogClient {
    fn parse(value: u8) -> Result<Self, XfsLogError> {
        match value {
            XFS_TRANSACTION_CLIENT => Ok(Self::Transaction),
            XFS_LOG_CLIENT => Ok(Self::Log),
            _ => Err(XfsLogError::InvalidData(format!(
                "invalid log operation client 0x{value:02X}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XfsLogOperationFlags(u8);

impl XfsLogOperationFlags {
    fn parse(value: u8) -> Result<Self, XfsLogError> {
        if value & !XLOG_KNOWN_FLAGS != 0 {
            return Err(XfsLogError::InvalidData(format!(
                "unknown log operation flags 0x{value:02X}"
            )));
        }
        Ok(Self(value))
    }

    pub fn bits(self) -> u8 {
        self.0
    }

    pub fn contains(self, flag: u8) -> bool {
        self.0 & flag != 0
    }

    pub fn starts_transaction(self) -> bool {
        self.contains(XLOG_START_TRANS)
    }

    pub fn commits_transaction(self) -> bool {
        self.contains(XLOG_COMMIT_TRANS)
    }

    pub fn is_continued_fragment(self) -> bool {
        self.contains(XLOG_CONTINUE_TRANS) || self.contains(XLOG_WAS_CONT_TRANS)
    }
}

#[derive(Debug, Clone)]
pub struct XfsLogOperation {
    pub transaction_id: u32,
    pub client: XfsLogClient,
    pub flags: XfsLogOperationFlags,
    pub reserved: u16,
    pub record_lsn: u64,
    pub record_log_block: u32,
    pub record_source_offset: u64,
    pub record_provenance: XfsLogRecordProvenance,
    pub record_checksum_status: XfsLogChecksumStatus,
    pub record_format: XfsLogFormat,
    pub operation_index: u32,
    pub region: Vec<u8>,
}

pub(crate) fn parse_log_operations(
    record: &XfsLogRecord,
) -> Result<Vec<XfsLogOperation>, XfsLogError> {
    let mut operations = Vec::with_capacity(record.header.operation_count as usize);
    let mut offset = 0usize;
    for operation_index in 0..record.header.operation_count {
        let header_end = offset
            .checked_add(XLOG_OP_HEADER_SIZE)
            .ok_or_else(|| XfsLogError::InvalidData("operation header offset overflows".into()))?;
        if header_end > record.body.len() {
            return Err(XfsLogError::InvalidData(format!(
                "operation {operation_index} header overruns record body"
            )));
        }
        let transaction_id = be_u32(&record.body, offset);
        let region_len = be_u32(&record.body, offset + 4) as usize;
        let client = XfsLogClient::parse(record.body[offset + 8])?;
        let flags = XfsLogOperationFlags::parse(record.body[offset + 9])?;
        let reserved = be_u16(&record.body, offset + 10);
        let region_end = header_end.checked_add(region_len).ok_or_else(|| {
            XfsLogError::InvalidData(format!(
                "operation {operation_index} region length overflows"
            ))
        })?;
        if region_end > record.body.len() {
            return Err(XfsLogError::InvalidData(format!(
                "operation {operation_index} declares {region_len} bytes beyond the record body"
            )));
        }
        if flags.starts_transaction() && region_len != 0 {
            return Err(XfsLogError::InvalidData(format!(
                "transaction start operation {operation_index} has non-zero region length {region_len}"
            )));
        }
        operations.push(XfsLogOperation {
            transaction_id,
            client,
            flags,
            reserved,
            record_lsn: record.header.lsn,
            record_log_block: record.log_block,
            record_source_offset: record.source_offset,
            record_provenance: record.provenance,
            record_checksum_status: record.checksum_status,
            record_format: record.header.format,
            operation_index,
            region: record.body[header_end..region_end].to_vec(),
        });
        offset = region_end;
    }
    Ok(operations)
}
