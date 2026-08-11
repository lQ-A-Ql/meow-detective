use super::record_validation::{
    read_log_span, record_provenance, restore_cycle_words, truncated_issue, validate_body_cycles,
    validate_checksum, validate_extended_header_cycles, validate_record_context, validate_snapshot,
};
use super::wire::{be_u32, be_u64, header_offset, le_u32, XfsLogFormat};
use super::{
    XfsLogError, XfsLogIssue, XfsLogIssueKind, XfsLogSnapshot, XLOG_BASIC_BLOCK_SIZE,
    XLOG_BIG_RECORD_BSIZE, XLOG_HEADER_CYCLE_SIZE, XLOG_HEADER_MAGIC_NUM, XLOG_MAX_RECORD_BSIZE,
    XLOG_MIN_RECORD_BSIZE, XLOG_OP_HEADER_SIZE,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRecordHeader {
    pub magic: u32,
    pub cycle: u32,
    pub version: u32,
    pub data_len: u32,
    pub lsn: u64,
    pub tail_lsn: u64,
    pub crc: u32,
    pub previous_block: u32,
    pub operation_count: u32,
    pub format: XfsLogFormat,
    pub fs_uuid: [u8; 16],
    pub iclog_size: u32,
}

impl LogRecordHeader {
    pub fn parse(data: &[u8]) -> Result<Self, XfsLogError> {
        if data.len() < XLOG_BASIC_BLOCK_SIZE {
            return Err(XfsLogError::InvalidData(format!(
                "log record header needs {XLOG_BASIC_BLOCK_SIZE} bytes, got {}",
                data.len()
            )));
        }
        let magic = be_u32(data, header_offset::MAGIC);
        if magic != XLOG_HEADER_MAGIC_NUM {
            return Err(XfsLogError::InvalidData(format!(
                "invalid log record magic 0x{magic:08X}, expected 0x{XLOG_HEADER_MAGIC_NUM:08X}"
            )));
        }
        let version = be_u32(data, header_offset::VERSION);
        if !matches!(version, 1 | 2) {
            return Err(XfsLogError::InvalidData(format!(
                "unsupported log record version {version}"
            )));
        }
        let data_len = be_u32(data, header_offset::DATA_LEN);
        if data_len == 0 || !(data_len as usize).is_multiple_of(8) {
            return Err(XfsLogError::InvalidData(format!(
                "log record data length {data_len} is zero or not 64-bit aligned"
            )));
        }
        let iclog_size = be_u32(data, header_offset::ICLOG_SIZE);
        validate_record_size(version, data_len, iclog_size)?;
        let operation_count = be_u32(data, header_offset::NUM_LOGOPS);
        if operation_count as usize > data_len as usize / XLOG_OP_HEADER_SIZE {
            return Err(XfsLogError::InvalidData(format!(
                "operation count {operation_count} cannot fit in {data_len} bytes"
            )));
        }

        let mut fs_uuid = [0u8; 16];
        fs_uuid.copy_from_slice(&data[header_offset::FS_UUID..header_offset::FS_UUID + 16]);
        Ok(Self {
            magic,
            cycle: be_u32(data, header_offset::CYCLE),
            version,
            data_len,
            lsn: be_u64(data, header_offset::LSN),
            tail_lsn: be_u64(data, header_offset::TAIL_LSN),
            crc: le_u32(data, header_offset::CRC),
            previous_block: be_u32(data, header_offset::PREV_BLOCK),
            operation_count,
            format: super::XfsLogFormat::from_raw(be_u32(data, header_offset::FORMAT)),
            fs_uuid,
            iclog_size,
        })
    }

    pub fn lsn_cycle(&self) -> u32 {
        (self.lsn >> 32) as u32
    }

    pub fn lsn_block(&self) -> u32 {
        self.lsn as u32
    }

    pub fn header_blocks(&self) -> usize {
        if self.version == 2 {
            (self.iclog_size as usize).div_ceil(XLOG_HEADER_CYCLE_SIZE)
        } else {
            1
        }
    }

    pub fn data_blocks(&self) -> usize {
        (self.data_len as usize).div_ceil(XLOG_BASIC_BLOCK_SIZE)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XfsLogChecksumStatus {
    Verified,
    NotPresent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XfsLogSourceSpan {
    pub snapshot_offset: u64,
    pub source_offset: u64,
    pub length: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XfsLogRecordProvenance {
    pub first: XfsLogSourceSpan,
    pub second: Option<XfsLogSourceSpan>,
}

impl XfsLogRecordProvenance {
    pub fn spans(self) -> impl Iterator<Item = XfsLogSourceSpan> {
        std::iter::once(self.first).chain(self.second)
    }
}

#[derive(Debug, Clone)]
pub struct XfsLogRecord {
    pub header: LogRecordHeader,
    pub log_block: u32,
    pub source_offset: u64,
    pub provenance: XfsLogRecordProvenance,
    pub checksum_status: XfsLogChecksumStatus,
    pub(crate) body: Vec<u8>,
}

pub(crate) struct RecordCollection {
    pub records: Vec<XfsLogRecord>,
    pub issues: Vec<XfsLogIssue>,
}

pub(crate) fn collect_log_records(
    snapshot: &XfsLogSnapshot,
    max_records: usize,
    max_body_bytes: u64,
) -> Result<RecordCollection, XfsLogError> {
    snapshot.geometry.validate()?;
    validate_snapshot(snapshot)?;
    let available_blocks = snapshot.bytes.len() / XLOG_BASIC_BLOCK_SIZE;
    let total_blocks = usize::try_from(snapshot.geometry.basic_block_count()?)
        .map_err(|_| XfsLogError::InvalidGeometry("log is too large to index".into()))?;
    let mut records = Vec::new();
    let mut issues = Vec::new();
    let mut total_body_bytes = 0u64;

    for block in 0..available_blocks {
        let offset = block * XLOG_BASIC_BLOCK_SIZE;
        if be_u32(&snapshot.bytes, offset) != XLOG_HEADER_MAGIC_NUM {
            continue;
        }
        if records.len() == max_records {
            issues.push(XfsLogIssue::new(
                XfsLogIssueKind::LimitReached,
                Some(block as u64),
                format!("record limit {max_records} reached"),
            ));
            break;
        }
        match decode_record(snapshot, block, total_blocks) {
            Ok(record) => {
                let new_total = total_body_bytes.saturating_add(record.body.len() as u64);
                if new_total > max_body_bytes {
                    issues.push(XfsLogIssue::new(
                        XfsLogIssueKind::LimitReached,
                        Some(block as u64),
                        format!("record body byte limit {max_body_bytes} reached"),
                    ));
                    break;
                }
                total_body_bytes = new_total;
                records.push(record);
            }
            Err(issue) => issues.push(issue),
        }
    }
    records.sort_by_key(|record| record.header.lsn);
    Ok(RecordCollection { records, issues })
}

fn validate_record_size(version: u32, data_len: u32, iclog_size: u32) -> Result<(), XfsLogError> {
    let data_len = data_len as usize;
    if version == 1 {
        if data_len > XLOG_BIG_RECORD_BSIZE {
            return Err(XfsLogError::InvalidData(format!(
                "v1 record length {data_len} exceeds {XLOG_BIG_RECORD_BSIZE}"
            )));
        }
        return Ok(());
    }

    let iclog_size = iclog_size as usize;
    if !(XLOG_MIN_RECORD_BSIZE..=XLOG_MAX_RECORD_BSIZE).contains(&iclog_size)
        || !iclog_size.is_power_of_two()
    {
        return Err(XfsLogError::InvalidData(format!(
            "v2 iclog size {iclog_size} is outside the supported power-of-two range {XLOG_MIN_RECORD_BSIZE}..={XLOG_MAX_RECORD_BSIZE}"
        )));
    }
    if data_len > iclog_size {
        return Err(XfsLogError::InvalidData(format!(
            "record length {data_len} exceeds v2 iclog size {iclog_size}"
        )));
    }
    Ok(())
}

fn decode_record(
    snapshot: &XfsLogSnapshot,
    block: usize,
    total_blocks: usize,
) -> Result<XfsLogRecord, XfsLogIssue> {
    let base_header =
        &snapshot.bytes[block * XLOG_BASIC_BLOCK_SIZE..(block + 1) * XLOG_BASIC_BLOCK_SIZE];
    let header = LogRecordHeader::parse(base_header).map_err(|error| {
        XfsLogIssue::new(
            XfsLogIssueKind::InvalidRecord,
            Some(block as u64),
            error.to_string(),
        )
    })?;
    validate_record_context(snapshot, &header, block, total_blocks)?;

    let header_bytes_len = header.header_blocks() * XLOG_BASIC_BLOCK_SIZE;
    let body_bytes_len = header.data_blocks() * XLOG_BASIC_BLOCK_SIZE;
    let span_blocks = header.header_blocks() + header.data_blocks();
    if span_blocks > total_blocks {
        return Err(XfsLogIssue::new(
            XfsLogIssueKind::InvalidRecord,
            Some(block as u64),
            format!("record spans {span_blocks} blocks in a {total_blocks}-block log"),
        ));
    }
    let header_bytes = read_log_span(snapshot, block * XLOG_BASIC_BLOCK_SIZE, header_bytes_len)
        .ok_or_else(|| truncated_issue(block, "extended record header"))?;
    validate_extended_header_cycles(&header, &header_bytes, block, total_blocks)?;
    let body_start = (block + header.header_blocks()) * XLOG_BASIC_BLOCK_SIZE;
    let mut body = read_log_span(snapshot, body_start, body_bytes_len)
        .ok_or_else(|| truncated_issue(block, "record body"))?;
    validate_body_cycles(&header, &body, block, total_blocks)?;
    let checksum_status = validate_checksum(
        snapshot,
        &header,
        &header_bytes,
        &body[..header.data_len as usize],
        block,
    )?;
    restore_cycle_words(&header, &header_bytes, &mut body, block, total_blocks)?;
    body.truncate(header.data_len as usize);
    let block_offset = u64::try_from(block)
        .ok()
        .and_then(|value| value.checked_mul(XLOG_BASIC_BLOCK_SIZE as u64))
        .ok_or_else(|| {
            XfsLogIssue::new(
                XfsLogIssueKind::InvalidRecord,
                Some(block as u64),
                "record source offset overflows",
            )
        })?;
    let source_offset = snapshot
        .source_offset
        .checked_add(block_offset)
        .ok_or_else(|| {
            XfsLogIssue::new(
                XfsLogIssueKind::InvalidRecord,
                Some(block as u64),
                "record source offset overflows",
            )
        })?;
    let provenance = record_provenance(
        snapshot,
        block * XLOG_BASIC_BLOCK_SIZE,
        header_bytes_len + body_bytes_len,
    )
    .ok_or_else(|| {
        XfsLogIssue::new(
            XfsLogIssueKind::InvalidRecord,
            Some(block as u64),
            "record provenance range overflows",
        )
    })?;
    Ok(XfsLogRecord {
        header,
        log_block: block as u32,
        source_offset,
        provenance,
        checksum_status,
        body,
    })
}
