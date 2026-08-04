use std::io::{Read, Seek, SeekFrom};

use sha2::{Digest, Sha256};

use crate::bytes::{read_checksum, read_u16, read_u32};
use crate::error::{Result, VolumeAndroidError};
use crate::geometry::LpGeometry;
use crate::LP_METADATA_HEADER_MAGIC;

const LP_METADATA_MAJOR_VERSION: u16 = 10;
const LP_METADATA_MINOR_VERSION_MAX: u16 = 2;
const LP_HEADER_V1_0_SIZE: usize = 128;
const LP_HEADER_V1_2_SIZE: usize = 256;
pub(super) const PARTITION_ENTRY_SIZE: u32 = 52;
pub(super) const EXTENT_ENTRY_SIZE: u32 = 24;
pub(super) const GROUP_ENTRY_SIZE: u32 = 48;
pub(super) const BLOCK_DEVICE_ENTRY_SIZE: u32 = 64;

#[derive(Debug, Clone, Copy)]
pub(super) struct TableDescriptor {
    pub(super) offset: u32,
    pub(super) num_entries: u32,
    pub(super) entry_size: u32,
}

#[derive(Debug)]
pub(super) struct MetadataHeader {
    pub(super) minor_version: u16,
    pub(super) header_size: usize,
    pub(super) tables_size: usize,
    pub(super) tables_checksum: [u8; 32],
    pub(super) partitions: TableDescriptor,
    pub(super) extents: TableDescriptor,
    pub(super) groups: TableDescriptor,
    pub(super) block_devices: TableDescriptor,
    pub(super) flags: u32,
}

pub(super) fn read_header_bytes<R: Read + Seek>(
    source: &mut R,
    offset: u64,
    geometry: LpGeometry,
    source_size: u64,
) -> Result<Vec<u8>> {
    let mut prefix = [0u8; LP_HEADER_V1_0_SIZE];
    read_exact_at(source, offset, &mut prefix, source_size)?;
    let header_size = read_u32(&prefix, 8, "metadata header size")? as usize;
    if !(LP_HEADER_V1_0_SIZE..=geometry.metadata_max_size as usize).contains(&header_size) {
        return Err(VolumeAndroidError::InvalidMetadata(format!(
            "header size {header_size} is outside the metadata slot"
        )));
    }
    let mut bytes = vec![0u8; header_size];
    read_exact_at(source, offset, &mut bytes, source_size)?;
    Ok(bytes)
}

pub(super) fn parse_header(bytes: &[u8], geometry: LpGeometry) -> Result<MetadataHeader> {
    if read_u32(bytes, 0, "metadata magic")? != LP_METADATA_HEADER_MAGIC {
        return Err(VolumeAndroidError::InvalidMetadata(
            "metadata header magic mismatch".to_string(),
        ));
    }
    let major = read_u16(bytes, 4, "metadata major version")?;
    let minor = read_u16(bytes, 6, "metadata minor version")?;
    if major != LP_METADATA_MAJOR_VERSION || minor > LP_METADATA_MINOR_VERSION_MAX {
        return Err(VolumeAndroidError::InvalidMetadata(format!(
            "unsupported metadata version {major}.{minor}"
        )));
    }
    if minor >= 2 && bytes.len() < LP_HEADER_V1_2_SIZE {
        return Err(VolumeAndroidError::InvalidMetadata(
            "metadata version 10.2 requires the expanded header".to_string(),
        ));
    }
    let tables_size = read_u32(bytes, 44, "metadata tables size")? as usize;
    if bytes.len().saturating_add(tables_size) > geometry.metadata_max_size as usize {
        return Err(VolumeAndroidError::InvalidMetadata(
            "header and tables exceed metadata max size".to_string(),
        ));
    }
    Ok(MetadataHeader {
        minor_version: minor,
        header_size: bytes.len(),
        tables_size,
        tables_checksum: read_checksum(bytes, 48)?,
        partitions: parse_descriptor(bytes, 80, "partition table")?,
        extents: parse_descriptor(bytes, 92, "extent table")?,
        groups: parse_descriptor(bytes, 104, "group table")?,
        block_devices: parse_descriptor(bytes, 116, "block-device table")?,
        flags: if bytes.len() >= 132 {
            read_u32(bytes, 128, "metadata header flags")?
        } else {
            0
        },
    })
}

pub(super) fn read_tables<R: Read + Seek>(
    source: &mut R,
    metadata_offset: u64,
    header: &MetadataHeader,
    geometry: LpGeometry,
    source_size: u64,
) -> Result<Vec<u8>> {
    if header.tables_size > geometry.metadata_max_size as usize - header.header_size {
        return Err(VolumeAndroidError::InvalidMetadata(
            "metadata table payload exceeds its slot".to_string(),
        ));
    }
    let tables_offset = metadata_offset
        .checked_add(header.header_size as u64)
        .ok_or(VolumeAndroidError::ArithmeticOverflow(
            "metadata table offset",
        ))?;
    let mut tables = vec![0u8; header.tables_size];
    read_exact_at(source, tables_offset, &mut tables, source_size)?;
    Ok(tables)
}

pub(super) fn validate_table_layout(header: &MetadataHeader, table_size: usize) -> Result<()> {
    let descriptors = [
        header.partitions,
        header.extents,
        header.groups,
        header.block_devices,
    ];
    let mut ranges = Vec::new();
    for descriptor in descriptors {
        let size = descriptor_size(descriptor)?;
        let start = descriptor.offset as usize;
        let end = start
            .checked_add(size)
            .ok_or(VolumeAndroidError::ArithmeticOverflow("table end"))?;
        if end > table_size {
            return Err(VolumeAndroidError::InvalidMetadata(
                "table descriptor exceeds the table payload".to_string(),
            ));
        }
        if size != 0 {
            ranges.push((start, end));
        }
    }
    ranges.sort_unstable();
    let mut expected_start = 0usize;
    for (start, end) in ranges {
        if start != expected_start {
            return Err(VolumeAndroidError::InvalidMetadata(
                "metadata tables contain a gap or overlap".to_string(),
            ));
        }
        expected_start = end;
    }
    if expected_start != table_size {
        return Err(VolumeAndroidError::InvalidMetadata(
            "metadata tables do not cover the declared payload".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn table_slice(bytes: &[u8], descriptor: TableDescriptor) -> Result<&[u8]> {
    let start = descriptor.offset as usize;
    let end = start
        .checked_add(descriptor_size(descriptor)?)
        .ok_or(VolumeAndroidError::ArithmeticOverflow("table slice end"))?;
    bytes
        .get(start..end)
        .ok_or_else(|| VolumeAndroidError::InvalidMetadata("table slice is invalid".to_string()))
}

pub(super) fn validate_entry_size(
    descriptor: TableDescriptor,
    expected: u32,
    table: &'static str,
) -> Result<()> {
    if descriptor.entry_size != expected {
        return Err(VolumeAndroidError::InvalidMetadata(format!(
            "{table} entry size {} does not match {expected}",
            descriptor.entry_size
        )));
    }
    Ok(())
}

pub(super) fn verify_header_checksum(bytes: &[u8]) -> Result<()> {
    let expected = read_checksum(bytes, 12)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes[..12]);
    hasher.update([0u8; 32]);
    hasher.update(&bytes[44..]);
    if hasher.finalize().as_slice() != expected {
        return Err(VolumeAndroidError::InvalidMetadata(
            "metadata header SHA-256 mismatch".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn verify_tables_checksum(bytes: &[u8], expected: [u8; 32]) -> Result<()> {
    if Sha256::digest(bytes).as_slice() != expected {
        return Err(VolumeAndroidError::InvalidMetadata(
            "metadata tables SHA-256 mismatch".to_string(),
        ));
    }
    Ok(())
}

fn parse_descriptor(bytes: &[u8], offset: usize, field: &'static str) -> Result<TableDescriptor> {
    Ok(TableDescriptor {
        offset: read_u32(bytes, offset, field)?,
        num_entries: read_u32(bytes, offset + 4, field)?,
        entry_size: read_u32(bytes, offset + 8, field)?,
    })
}

fn descriptor_size(descriptor: TableDescriptor) -> Result<usize> {
    let size = descriptor
        .num_entries
        .checked_mul(descriptor.entry_size)
        .ok_or(VolumeAndroidError::ArithmeticOverflow("table size"))?;
    if size > i32::MAX as u32 {
        return Err(VolumeAndroidError::InvalidMetadata(
            "table size exceeds the liblp signed bound".to_string(),
        ));
    }
    Ok(size as usize)
}

fn read_exact_at<R: Read + Seek>(
    source: &mut R,
    offset: u64,
    buffer: &mut [u8],
    source_size: u64,
) -> Result<()> {
    let end = offset
        .checked_add(buffer.len() as u64)
        .ok_or(VolumeAndroidError::ArithmeticOverflow("read end"))?;
    if end > source_size {
        return Err(VolumeAndroidError::Truncated("metadata copy"));
    }
    source.seek(SeekFrom::Start(offset))?;
    source.read_exact(buffer)?;
    Ok(())
}
