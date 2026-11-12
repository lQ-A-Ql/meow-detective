use super::kubernetes_parser_error::{KubernetesParserError, Result};

const MAX_ETCD_WAL_BYTES: usize = 512 * 1024 * 1024;
const MAX_ETCD_WAL_RECORDS: usize = 131_072;
const MAX_ETCD_WAL_PAYLOAD: usize = 16 * 1024 * 1024;
const RECORD_HEADER_SIZE: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EtcdWalSummary {
    pub records: Vec<EtcdWalRecord>,
    pub metadata_count: usize,
    pub entry_count: usize,
    pub state_count: usize,
    pub checksum_failures: usize,
    pub truncated_tail: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EtcdWalRecord {
    pub offset: u64,
    pub record_type: u8,
    pub payload_length: usize,
    pub checksum_valid: bool,
    pub protobuf_field_count: usize,
}

pub fn parse_etcd_wal(input: &[u8]) -> Result<EtcdWalSummary> {
    if input.len() > MAX_ETCD_WAL_BYTES {
        return Err(KubernetesParserError::Limit {
            kind: "etcd WAL bytes",
            actual: input.len(),
            max: MAX_ETCD_WAL_BYTES,
        });
    }
    let mut records = Vec::new();
    let mut offset = 0usize;
    let mut previous_crc = 0u32;
    let mut checksum_failures = 0;
    let mut truncated_tail = false;
    while offset < input.len() {
        if input[offset..].iter().all(|byte| *byte == 0) {
            break;
        }
        if input.len() - offset < RECORD_HEADER_SIZE {
            truncated_tail = true;
            break;
        }
        let stored_crc = u32::from_le_bytes([
            input[offset],
            input[offset + 1],
            input[offset + 2],
            input[offset + 3],
        ]);
        let payload_length = u16::from_le_bytes([input[offset + 4], input[offset + 5]]) as usize;
        let record_type = input[offset + 6];
        if payload_length > MAX_ETCD_WAL_PAYLOAD {
            return Err(KubernetesParserError::Limit {
                kind: "etcd WAL payload",
                actual: payload_length,
                max: MAX_ETCD_WAL_PAYLOAD,
            });
        }
        let payload_start = offset + RECORD_HEADER_SIZE;
        let payload_end = payload_start.checked_add(payload_length).ok_or(
            KubernetesParserError::InvalidBinary {
                offset: offset as u64,
                reason: "WAL payload overflow".to_string(),
            },
        )?;
        if payload_end > input.len() {
            truncated_tail = true;
            break;
        }
        let frame_length = align8(RECORD_HEADER_SIZE + payload_length);
        if offset + frame_length > input.len() {
            truncated_tail = true;
            break;
        }
        let payload = &input[payload_start..payload_end];
        let calculated = ceph_wire::crc32c::crc32c(previous_crc, payload);
        let checksum_valid = calculated == stored_crc;
        if !checksum_valid {
            checksum_failures += 1;
        }
        previous_crc = if checksum_valid {
            stored_crc
        } else {
            calculated
        };
        let protobuf_field_count = protobuf_field_count(payload)?;
        records.push(EtcdWalRecord {
            offset: offset as u64,
            record_type,
            payload_length,
            checksum_valid,
            protobuf_field_count,
        });
        if records.len() >= MAX_ETCD_WAL_RECORDS {
            return Err(KubernetesParserError::Limit {
                kind: "etcd WAL records",
                actual: records.len() + 1,
                max: MAX_ETCD_WAL_RECORDS,
            });
        }
        offset += frame_length;
    }
    Ok(EtcdWalSummary {
        metadata_count: records
            .iter()
            .filter(|record| record.record_type == 1)
            .count(),
        entry_count: records
            .iter()
            .filter(|record| record.record_type == 2)
            .count(),
        state_count: records
            .iter()
            .filter(|record| record.record_type == 3)
            .count(),
        records,
        checksum_failures,
        truncated_tail,
    })
}

fn align8(value: usize) -> usize {
    (value + 7) & !7
}

fn protobuf_field_count(payload: &[u8]) -> Result<usize> {
    let mut offset = 0usize;
    let mut fields = 0usize;
    while offset < payload.len() {
        let key = read_varint(payload, &mut offset)?;
        let wire_type = key & 0x07;
        if key >> 3 == 0 {
            return Err(KubernetesParserError::InvalidBinary {
                offset: offset as u64,
                reason: "protobuf field number is zero".to_string(),
            });
        }
        match wire_type {
            0 => {
                let _ = read_varint(payload, &mut offset)?;
            }
            1 => skip(payload, &mut offset, 8)?,
            2 => {
                let length = read_varint(payload, &mut offset)? as usize;
                skip(payload, &mut offset, length)?;
            }
            5 => skip(payload, &mut offset, 4)?,
            _ => {
                return Err(KubernetesParserError::InvalidBinary {
                    offset: offset as u64,
                    reason: format!("unsupported protobuf wire type {wire_type}"),
                })
            }
        }
        fields += 1;
    }
    Ok(fields)
}

fn read_varint(bytes: &[u8], offset: &mut usize) -> Result<u64> {
    let mut value = 0u64;
    for shift in (0..70).step_by(7) {
        let byte = *bytes.get(*offset).ok_or(KubernetesParserError::Truncated {
            offset: *offset as u64,
            context: "protobuf varint",
        })?;
        *offset += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(KubernetesParserError::InvalidBinary {
        offset: *offset as u64,
        reason: "protobuf varint overflow".to_string(),
    })
}

fn skip(bytes: &[u8], offset: &mut usize, length: usize) -> Result<()> {
    *offset = offset
        .checked_add(length)
        .ok_or(KubernetesParserError::InvalidBinary {
            offset: *offset as u64,
            reason: "protobuf field length overflow".to_string(),
        })?;
    if *offset > bytes.len() {
        return Err(KubernetesParserError::Truncated {
            offset: *offset as u64,
            context: "protobuf field",
        });
    }
    Ok(())
}
