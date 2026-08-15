use crate::crc32c::{extend_crc32c, unmask_crc32c};
use crate::error::{Result, RocksDbWireError};
use crate::limits::LogDecodeLimits;

pub const ROCKSDB_LOG_BLOCK_SIZE: usize = 32_768;
pub(crate) const ROCKSDB_LOG_HEADER_SIZE: usize = 7;
pub(crate) const ROCKSDB_RECYCLABLE_LOG_HEADER_SIZE: usize = 11;

const ZERO_TYPE: u8 = 0;
const FULL_TYPE: u8 = 1;
const FIRST_TYPE: u8 = 2;
const MIDDLE_TYPE: u8 = 3;
const LAST_TYPE: u8 = 4;
const RECYCLABLE_FULL_TYPE: u8 = 5;
const RECYCLABLE_FIRST_TYPE: u8 = 6;
const RECYCLABLE_MIDDLE_TYPE: u8 = 7;
const RECYCLABLE_LAST_TYPE: u8 = 8;
const SET_COMPRESSION_TYPE: u8 = 9;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LogDecodeOptions {
    pub expected_recyclable_log_number: Option<u32>,
    pub limits: LogDecodeLimits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalLogRecord {
    pub ordinal: u64,
    pub physical_offset: u64,
    pub recyclable_log_number: Option<u32>,
    pub fragment_count: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FragmentKind {
    Full,
    First,
    Middle,
    Last,
}

#[derive(Debug)]
struct PhysicalRecord<'a> {
    offset: usize,
    kind: FragmentKind,
    recyclable_log_number: Option<u32>,
    payload: &'a [u8],
}

#[derive(Debug)]
struct Assembly {
    offset: usize,
    recyclable_log_number: Option<u32>,
    fragment_count: u32,
    data: Vec<u8>,
}

pub fn decode_log(input: &[u8], options: LogDecodeOptions) -> Result<Vec<LogicalLogRecord>> {
    if input.len() > options.limits.max_file_bytes {
        return Err(RocksDbWireError::LogLengthLimit {
            length: input.len(),
            limit: options.limits.max_file_bytes,
        });
    }

    let mut records = Vec::new();
    let mut assembly = None;
    for (block_index, block) in input.chunks(ROCKSDB_LOG_BLOCK_SIZE).enumerate() {
        let block_start = block_index.checked_mul(ROCKSDB_LOG_BLOCK_SIZE).ok_or(
            RocksDbWireError::LengthOverflow {
                context: "RocksDB log block offset",
            },
        )?;
        decode_block(
            block,
            block_start,
            input.len(),
            &options,
            &mut assembly,
            &mut records,
        )?;
    }

    if let Some(partial) = assembly {
        return Err(RocksDbWireError::UnterminatedLogicalRecord {
            offset: partial.offset,
        });
    }
    Ok(records)
}

fn decode_block(
    block: &[u8],
    block_start: usize,
    file_length: usize,
    options: &LogDecodeOptions,
    assembly: &mut Option<Assembly>,
    records: &mut Vec<LogicalLogRecord>,
) -> Result<()> {
    let mut position = 0usize;
    while position < block.len() {
        let remaining = block.len() - position;
        if remaining < ROCKSDB_LOG_HEADER_SIZE {
            validate_trailer(&block[position..], block_start + position)?;
            break;
        }

        if is_zero_record(&block[position..]) {
            validate_zero_record(&block[position..], block_start + position)?;
            break;
        }

        let physical = parse_physical_record(
            block,
            block_start,
            file_length,
            position,
            options.expected_recyclable_log_number,
        )?;
        let consumed = record_header_size(physical.recyclable_log_number)
            .checked_add(physical.payload.len())
            .ok_or(RocksDbWireError::LengthOverflow {
                context: "physical record length",
            })?;
        consume_fragment(physical, options.limits, assembly, records)?;
        position = position
            .checked_add(consumed)
            .ok_or(RocksDbWireError::LengthOverflow {
                context: "physical record position",
            })?;
    }
    Ok(())
}

fn parse_physical_record<'a>(
    block: &'a [u8],
    block_start: usize,
    file_length: usize,
    position: usize,
    expected_log_number: Option<u32>,
) -> Result<PhysicalRecord<'a>> {
    let offset = block_start + position;
    let record_type = block[position + 6];
    let (kind, recyclable) = decode_record_type(record_type, offset)?;
    let header_size = if recyclable {
        ROCKSDB_RECYCLABLE_LOG_HEADER_SIZE
    } else {
        ROCKSDB_LOG_HEADER_SIZE
    };
    let block_remaining = block.len() - position;
    if block_remaining < header_size {
        return Err(RocksDbWireError::TruncatedLogHeader {
            offset,
            available: block_remaining,
        });
    }

    let payload_length = usize::from(u16::from_le_bytes([
        block[position + 4],
        block[position + 5],
    ]));
    validate_physical_length(
        offset,
        block_start,
        block.len(),
        file_length,
        header_size,
        payload_length,
        block_remaining,
    )?;
    let log_number =
        decode_recyclable_log_number(block, position, offset, recyclable, expected_log_number)?;
    let payload_start = position + header_size;
    let payload_end = payload_start + payload_length;
    validate_crc(
        &block[position..payload_end],
        header_size,
        payload_length,
        offset,
    )?;
    Ok(PhysicalRecord {
        offset,
        kind,
        recyclable_log_number: log_number,
        payload: &block[payload_start..payload_end],
    })
}

fn validate_physical_length(
    offset: usize,
    block_start: usize,
    block_length: usize,
    file_length: usize,
    header_size: usize,
    payload_length: usize,
    block_remaining: usize,
) -> Result<()> {
    let total =
        header_size
            .checked_add(payload_length)
            .ok_or(RocksDbWireError::LengthOverflow {
                context: "physical record length",
            })?;
    if total <= block_remaining {
        return Ok(());
    }
    let block_end = block_start + block_length;
    if block_length == ROCKSDB_LOG_BLOCK_SIZE || block_end < file_length {
        return Err(RocksDbWireError::CrossBlockRecord {
            offset,
            header_size,
            payload_length,
            block_remaining,
        });
    }
    Err(RocksDbWireError::TruncatedLogBody {
        offset,
        declared: payload_length,
        available: block_remaining.saturating_sub(header_size),
    })
}

fn decode_recyclable_log_number(
    block: &[u8],
    position: usize,
    offset: usize,
    recyclable: bool,
    expected: Option<u32>,
) -> Result<Option<u32>> {
    if !recyclable {
        return Ok(None);
    }
    let actual =
        u32::from_le_bytes(block[position + 7..position + 11].try_into().map_err(|_| {
            RocksDbWireError::TruncatedLogHeader {
                offset,
                available: block.len() - position,
            }
        })?);
    let expected = expected.ok_or(RocksDbWireError::RecyclableLogNumberRequired { offset })?;
    if actual != expected {
        return Err(RocksDbWireError::RecyclableLogNumberMismatch {
            offset,
            expected,
            actual,
        });
    }
    Ok(Some(actual))
}

fn validate_crc(
    record: &[u8],
    header_size: usize,
    payload_length: usize,
    offset: usize,
) -> Result<()> {
    let stored = u32::from_le_bytes(record[0..4].try_into().map_err(|_| {
        RocksDbWireError::TruncatedLogHeader {
            offset,
            available: record.len(),
        }
    })?);
    let expected = unmask_crc32c(stored);
    let header_crc = extend_crc32c(0, &record[6..header_size]);
    let actual = extend_crc32c(
        header_crc,
        &record[header_size..header_size + payload_length],
    );
    if actual != expected {
        return Err(RocksDbWireError::LogCrcMismatch {
            offset,
            expected,
            actual,
        });
    }
    Ok(())
}

fn consume_fragment(
    physical: PhysicalRecord<'_>,
    limits: LogDecodeLimits,
    assembly: &mut Option<Assembly>,
    records: &mut Vec<LogicalLogRecord>,
) -> Result<()> {
    match physical.kind {
        FragmentKind::Full => {
            if assembly.is_some() {
                return sequence_error(&physical, "MIDDLE or LAST", "FULL");
            }
            ensure_record_length(physical.payload.len(), limits)?;
            push_record(
                records,
                limits,
                physical.offset,
                physical.recyclable_log_number,
                1,
                physical.payload.to_vec(),
            )
        }
        FragmentKind::First => {
            if assembly.is_some() {
                return sequence_error(&physical, "MIDDLE or LAST", "FIRST");
            }
            ensure_record_length(physical.payload.len(), limits)?;
            *assembly = Some(Assembly {
                offset: physical.offset,
                recyclable_log_number: physical.recyclable_log_number,
                fragment_count: 1,
                data: physical.payload.to_vec(),
            });
            Ok(())
        }
        FragmentKind::Middle => append_fragment(physical, limits, assembly, false, records),
        FragmentKind::Last => append_fragment(physical, limits, assembly, true, records),
    }
}

fn append_fragment(
    physical: PhysicalRecord<'_>,
    limits: LogDecodeLimits,
    assembly: &mut Option<Assembly>,
    is_last: bool,
    records: &mut Vec<LogicalLogRecord>,
) -> Result<()> {
    let expected = "FIRST or MIDDLE";
    let actual = if is_last { "LAST" } else { "MIDDLE" };
    let partial = assembly
        .as_mut()
        .ok_or(RocksDbWireError::InvalidFragmentSequence {
            offset: physical.offset,
            expected,
            actual,
        })?;
    if partial.recyclable_log_number != physical.recyclable_log_number {
        return Err(RocksDbWireError::MixedFragmentEncoding {
            offset: physical.offset,
        });
    }
    let new_length = partial
        .data
        .len()
        .checked_add(physical.payload.len())
        .ok_or(RocksDbWireError::LengthOverflow {
            context: "logical record length",
        })?;
    ensure_record_length(new_length, limits)?;
    partial.data.extend_from_slice(physical.payload);
    partial.fragment_count += 1;
    if is_last {
        let completed = assembly.take().ok_or(RocksDbWireError::LengthOverflow {
            context: "logical record assembly",
        })?;
        push_record(
            records,
            limits,
            completed.offset,
            completed.recyclable_log_number,
            completed.fragment_count,
            completed.data,
        )?;
    }
    Ok(())
}

fn push_record(
    records: &mut Vec<LogicalLogRecord>,
    limits: LogDecodeLimits,
    offset: usize,
    log_number: Option<u32>,
    fragment_count: u32,
    data: Vec<u8>,
) -> Result<()> {
    if records.len() >= limits.max_logical_records {
        return Err(RocksDbWireError::LogicalRecordCountLimit {
            limit: limits.max_logical_records,
        });
    }
    records.push(LogicalLogRecord {
        ordinal: records.len() as u64,
        physical_offset: offset as u64,
        recyclable_log_number: log_number,
        fragment_count,
        data,
    });
    Ok(())
}

fn ensure_record_length(length: usize, limits: LogDecodeLimits) -> Result<()> {
    if length > limits.max_logical_record_bytes {
        return Err(RocksDbWireError::LogicalRecordLengthLimit {
            length,
            limit: limits.max_logical_record_bytes,
        });
    }
    Ok(())
}

fn decode_record_type(record_type: u8, offset: usize) -> Result<(FragmentKind, bool)> {
    let decoded = match record_type {
        FULL_TYPE => (FragmentKind::Full, false),
        FIRST_TYPE => (FragmentKind::First, false),
        MIDDLE_TYPE => (FragmentKind::Middle, false),
        LAST_TYPE => (FragmentKind::Last, false),
        RECYCLABLE_FULL_TYPE => (FragmentKind::Full, true),
        RECYCLABLE_FIRST_TYPE => (FragmentKind::First, true),
        RECYCLABLE_MIDDLE_TYPE => (FragmentKind::Middle, true),
        RECYCLABLE_LAST_TYPE => (FragmentKind::Last, true),
        SET_COMPRESSION_TYPE => {
            return Err(RocksDbWireError::UnsupportedWalCompressionRecord { offset });
        }
        _ => {
            return Err(RocksDbWireError::InvalidLogRecordType {
                offset,
                record_type,
            });
        }
    };
    Ok(decoded)
}

fn validate_trailer(trailer: &[u8], offset: usize) -> Result<()> {
    if trailer.iter().any(|byte| *byte != 0) {
        return Err(RocksDbWireError::NonZeroLogTrailer { offset });
    }
    Ok(())
}

fn is_zero_record(bytes: &[u8]) -> bool {
    bytes[6] == ZERO_TYPE
        && bytes[4] == 0
        && bytes[5] == 0
        && bytes[0..4].iter().all(|byte| *byte == 0)
}

fn validate_zero_record(bytes: &[u8], offset: usize) -> Result<()> {
    if bytes.iter().any(|byte| *byte != 0) {
        return Err(RocksDbWireError::InvalidZeroRecord { offset });
    }
    Ok(())
}

fn record_header_size(log_number: Option<u32>) -> usize {
    if log_number.is_some() {
        ROCKSDB_RECYCLABLE_LOG_HEADER_SIZE
    } else {
        ROCKSDB_LOG_HEADER_SIZE
    }
}

fn sequence_error<T>(
    physical: &PhysicalRecord<'_>,
    expected: &'static str,
    actual: &'static str,
) -> Result<T> {
    Err(RocksDbWireError::InvalidFragmentSequence {
        offset: physical.offset,
        expected,
        actual,
    })
}
