use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, SeekFrom};

use evidence_core::EvidenceReader;
use persistence_sqlite::repositories::ceph_bluestore_semantic_repo::{
    CephBluestoreBlobRecord, CephBluestoreChecksumChunkRecord, CephBluestoreObjectReadPlan,
    CephBluestorePhysicalExtentRecord,
};

use super::{invalid_data, RadosReadError};

#[derive(Clone)]
pub(super) struct LogicalExtent {
    pub(super) logical_offset: u64,
    pub(super) end: u64,
    pub(super) blob_ordinal: u32,
    pub(super) blob_offset: u64,
}

#[derive(Clone)]
pub(super) struct BlobPlan {
    logical_length: u64,
    physical_extents: Vec<PhysicalExtent>,
    checksum_type: Option<ChecksumType>,
    checksums: Vec<ChecksumChunk>,
}

impl BlobPlan {
    pub(super) fn read_range(
        &self,
        device: &mut dyn EvidenceReader,
        offset: u64,
        output: &mut [u8],
    ) -> io::Result<()> {
        let end = offset
            .checked_add(output.len() as u64)
            .ok_or_else(|| invalid_data("blob range end overflow"))?;
        if end > self.logical_length {
            return Err(invalid_data("blob range exceeds logical length"));
        }
        output.fill(0);
        for extent in &self.physical_extents {
            if extent.blob_offset >= end || extent.end <= offset {
                continue;
            }
            let start = offset.max(extent.blob_offset);
            let extent_end = end.min(extent.end);
            let destination = usize::try_from(start - offset)
                .map_err(|_| invalid_data("blob destination exceeds usize"))?;
            let length = usize::try_from(extent_end - start)
                .map_err(|_| invalid_data("physical read length exceeds usize"))?;
            let Some(physical_offset) = extent.physical_offset else {
                continue;
            };
            let source = physical_offset
                .checked_add(start - extent.blob_offset)
                .ok_or_else(|| invalid_data("physical read offset overflow"))?;
            device.seek(SeekFrom::Start(source))?;
            device.read_exact(&mut output[destination..][..length])?;
        }
        Ok(())
    }

    pub(super) fn verify_overlapping_checksums(
        &self,
        device: &mut dyn EvidenceReader,
        offset: u64,
        output: &[u8],
    ) -> io::Result<()> {
        let Some(checksum_type) = self.checksum_type else {
            return Ok(());
        };
        let end = offset
            .checked_add(output.len() as u64)
            .ok_or_else(|| invalid_data("checksum range end overflow"))?;
        let mut verified = BTreeSet::new();
        for checksum in &self.checksums {
            if checksum.offset >= end
                || checksum.end <= offset
                || !verified.insert(checksum.ordinal)
            {
                continue;
            }
            let actual = if checksum.offset >= offset && checksum.end <= end {
                let start = usize::try_from(checksum.offset - offset)
                    .map_err(|_| invalid_data("checksum output offset exceeds usize"))?;
                let end = usize::try_from(checksum.end - offset)
                    .map_err(|_| invalid_data("checksum output end exceeds usize"))?;
                checksum_type.calculate(&output[start..end])
            } else {
                let chunk_length = checksum.end.min(self.logical_length) - checksum.offset;
                let mut bytes = vec![
                    0;
                    usize::try_from(chunk_length).map_err(|_| {
                        invalid_data("checksum chunk exceeds usize")
                    })?
                ];
                self.read_range(device, checksum.offset, &mut bytes)?;
                checksum_type.calculate(&bytes)
            };
            if actual != checksum.value {
                return Err(invalid_data(format!(
                    "BlueStore checksum mismatch at blob offset {}",
                    checksum.offset
                )));
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
struct PhysicalExtent {
    blob_offset: u64,
    end: u64,
    physical_offset: Option<u64>,
}

#[derive(Clone)]
struct ChecksumChunk {
    ordinal: u32,
    offset: u64,
    end: u64,
    value: u64,
}

#[derive(Clone, Copy)]
enum ChecksumType {
    Crc32c,
    Crc32c16,
    Crc32c8,
}

impl ChecksumType {
    fn parse(value: &str) -> Result<Self, RadosReadError> {
        match value {
            "crc32c" => Ok(Self::Crc32c),
            "crc32c16" => Ok(Self::Crc32c16),
            "crc32c8" => Ok(Self::Crc32c8),
            "xxHash32" | "xxHash64" => Err(RadosReadError::Unsupported(format!(
                "checksum algorithm {value} is not available in the bounded reader"
            ))),
            _ => Err(RadosReadError::InvalidPlan(format!(
                "unknown checksum algorithm {value}"
            ))),
        }
    }

    fn calculate(self, bytes: &[u8]) -> u64 {
        let checksum = ceph_wire::crc32c::ceph_crc32c(bytes);
        match self {
            Self::Crc32c => u64::from(checksum),
            Self::Crc32c16 => u64::from(checksum & 0xffff),
            Self::Crc32c8 => u64::from(checksum & 0xff),
        }
    }
}

pub(super) fn build_blob_plans(
    plan: &CephBluestoreObjectReadPlan,
) -> Result<BTreeMap<u32, BlobPlan>, RadosReadError> {
    if plan.object.blob_count != plan.blobs.len() as u64
        || plan.object.logical_extent_count != plan.logical_extents.len() as u64
        || plan.object.physical_extent_count != plan.physical_extents.len() as u64
    {
        return Err(RadosReadError::InvalidPlan(
            "object child counts do not match the read plan".to_string(),
        ));
    }
    let mut result = BTreeMap::new();
    for blob in &plan.blobs {
        validate_blob(blob)?;
        let physical_extents = physical_extents_for_blob(&plan.physical_extents, blob)?;
        let checksums = checksums_for_blob(&plan.checksum_chunks, blob)?;
        let checksum_type = blob
            .checksum_type
            .as_deref()
            .map(ChecksumType::parse)
            .transpose()?;
        let row = BlobPlan {
            logical_length: blob.logical_length,
            physical_extents,
            checksum_type,
            checksums,
        };
        if result.insert(blob.blob_ordinal, row).is_some() {
            return Err(RadosReadError::InvalidPlan(
                "duplicate blob ordinal".to_string(),
            ));
        }
    }
    Ok(result)
}

fn validate_blob(blob: &CephBluestoreBlobRecord) -> Result<(), RadosReadError> {
    if blob.flag_compressed {
        return Err(RadosReadError::Unsupported(
            "compressed BlueStore blobs require algorithm-specific decoding".to_string(),
        ));
    }
    if blob.flags_unknown_bits != 0 {
        return Err(RadosReadError::Unsupported(
            "BlueStore blob contains unknown flags".to_string(),
        ));
    }
    Ok(())
}

fn physical_extents_for_blob(
    rows: &[CephBluestorePhysicalExtentRecord],
    blob: &CephBluestoreBlobRecord,
) -> Result<Vec<PhysicalExtent>, RadosReadError> {
    let mut result = rows
        .iter()
        .filter(|row| row.blob_ordinal == blob.blob_ordinal)
        .map(|row| {
            if row.device_id != 1 {
                return Err(RadosReadError::Unsupported(format!(
                    "BlueStore physical extent references device {}",
                    row.device_id
                )));
            }
            let end = row.blob_offset.checked_add(row.length).ok_or_else(|| {
                RadosReadError::InvalidPlan("physical extent end overflow".to_string())
            })?;
            let physical_offset = row
                .physical_offset_hex
                .as_deref()
                .map(|value| u64::from_str_radix(value, 16))
                .transpose()
                .map_err(|_| {
                    RadosReadError::InvalidPlan("physical extent offset is not hex".to_string())
                })?;
            Ok(PhysicalExtent {
                blob_offset: row.blob_offset,
                end,
                physical_offset,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    result.sort_by_key(|row| row.blob_offset);
    let mut previous_end = 0;
    for extent in &result {
        if extent.blob_offset != previous_end {
            return Err(RadosReadError::InvalidPlan(
                "physical extents do not form a canonical blob map".to_string(),
            ));
        }
        previous_end = extent.end;
    }
    if previous_end != blob.on_disk_length {
        return Err(RadosReadError::InvalidPlan(
            "physical extents do not close to the blob on-disk length".to_string(),
        ));
    }
    Ok(result)
}

fn checksums_for_blob(
    rows: &[CephBluestoreChecksumChunkRecord],
    blob: &CephBluestoreBlobRecord,
) -> Result<Vec<ChecksumChunk>, RadosReadError> {
    let mut result = rows
        .iter()
        .filter(|row| row.blob_ordinal == blob.blob_ordinal)
        .map(|row| {
            let end = row
                .chunk_offset
                .checked_add(row.chunk_length)
                .ok_or_else(|| {
                    RadosReadError::InvalidPlan("checksum chunk end overflow".to_string())
                })?;
            Ok(ChecksumChunk {
                ordinal: row.checksum_ordinal,
                offset: row.chunk_offset,
                end,
                value: row.checksum_value,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    result.sort_by_key(|row| row.ordinal);
    if blob.flag_checksum != blob.checksum_type.is_some()
        || (!blob.flag_checksum && !result.is_empty())
        || (blob.flag_checksum && result.len() as u64 != blob.checksum_value_count)
    {
        return Err(RadosReadError::InvalidPlan(
            "checksum rows do not match the blob declaration".to_string(),
        ));
    }
    let mut previous_end = 0;
    for checksum in &result {
        if checksum.offset != previous_end
            || checksum.end <= checksum.offset
            || checksum.end > blob.logical_length
        {
            return Err(RadosReadError::InvalidPlan(
                "checksum chunks do not form a canonical blob map".to_string(),
            ));
        }
        previous_end = checksum.end;
    }
    if blob.flag_checksum && previous_end != blob.logical_length {
        return Err(RadosReadError::InvalidPlan(
            "checksum chunks do not cover the complete logical blob".to_string(),
        ));
    }
    Ok(result)
}

pub(super) fn build_logical_extents(
    plan: &CephBluestoreObjectReadPlan,
    blobs: &BTreeMap<u32, BlobPlan>,
) -> Result<Vec<LogicalExtent>, RadosReadError> {
    let mut result = Vec::with_capacity(plan.logical_extents.len());
    let mut previous_end = 0;
    for row in &plan.logical_extents {
        let end = row.logical_offset.checked_add(row.length).ok_or_else(|| {
            RadosReadError::InvalidPlan("logical extent end overflow".to_string())
        })?;
        if row.logical_offset < previous_end || end > plan.object.size {
            return Err(RadosReadError::InvalidPlan(
                "logical extents overlap or exceed the object".to_string(),
            ));
        }
        let blob = blobs.get(&row.blob_ordinal).ok_or_else(|| {
            RadosReadError::InvalidPlan("logical extent references a missing blob".to_string())
        })?;
        if row
            .blob_offset
            .checked_add(row.length)
            .is_none_or(|value| value > blob.logical_length)
        {
            return Err(RadosReadError::InvalidPlan(
                "logical extent exceeds its blob".to_string(),
            ));
        }
        result.push(LogicalExtent {
            logical_offset: row.logical_offset,
            end,
            blob_ordinal: row.blob_ordinal,
            blob_offset: row.blob_offset,
        });
        previous_end = end;
    }
    Ok(result)
}
