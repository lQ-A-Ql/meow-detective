mod header;
mod tables;
mod validation;

use std::io::{Read, Seek, SeekFrom};

use crate::error::{Result, VolumeAndroidError};
use crate::geometry::LpGeometry;
use header::{
    parse_header, read_header_bytes, read_tables, table_slice, validate_entry_size,
    validate_table_layout, verify_header_checksum, verify_tables_checksum, BLOCK_DEVICE_ENTRY_SIZE,
    EXTENT_ENTRY_SIZE, GROUP_ENTRY_SIZE, PARTITION_ENTRY_SIZE,
};
use tables::{parse_block_devices, parse_extents, parse_partitions};
use validation::{validate_physical_extents, validate_super_device};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataCopy {
    Primary,
    Backup,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockDevice {
    pub first_logical_sector: u64,
    pub alignment: u32,
    pub alignment_offset: u32,
    pub size: u64,
    pub partition_name: String,
    pub flags: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalExtentTarget {
    Linear {
        source_index: u32,
        source_offset: u64,
    },
    Zero,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogicalExtent {
    pub logical_offset: u64,
    pub length: u64,
    pub target: LogicalExtentTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalPartition {
    pub name: String,
    pub attributes: u32,
    pub group_index: u32,
    pub size: u64,
    pub disabled: bool,
    pub extents: Vec<LogicalExtent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuperMetadata {
    pub geometry: LpGeometry,
    pub slot_number: u32,
    pub source_copy: MetadataCopy,
    pub minor_version: u16,
    pub header_flags: u32,
    pub partitions: Vec<LogicalPartition>,
    pub block_devices: Vec<BlockDevice>,
}

impl SuperMetadata {
    pub fn read_slot<R: Read + Seek>(source: &mut R, slot_number: u32) -> Result<Self> {
        let geometry = LpGeometry::read(source)?;
        let primary_offset = geometry.primary_metadata_offset(slot_number)?;
        match read_metadata_copy(
            source,
            geometry,
            slot_number,
            primary_offset,
            MetadataCopy::Primary,
        ) {
            Ok(metadata) => Ok(metadata),
            Err(primary) => {
                let backup_offset = geometry.backup_metadata_offset(slot_number)?;
                read_metadata_copy(
                    source,
                    geometry,
                    slot_number,
                    backup_offset,
                    MetadataCopy::Backup,
                )
                .map_err(|backup| VolumeAndroidError::MetadataCopiesInvalid {
                    slot: slot_number,
                    primary: primary.to_string(),
                    backup: backup.to_string(),
                })
            }
        }
    }

    pub fn partition(&self, name: &str) -> Option<&LogicalPartition> {
        self.partitions
            .iter()
            .find(|partition| partition.name == name)
    }
}

fn read_metadata_copy<R: Read + Seek>(
    source: &mut R,
    geometry: LpGeometry,
    slot_number: u32,
    offset: u64,
    source_copy: MetadataCopy,
) -> Result<SuperMetadata> {
    let source_size = source.seek(SeekFrom::End(0))?;
    let header_bytes = read_header_bytes(source, offset, geometry, source_size)?;
    let header = parse_header(&header_bytes, geometry)?;
    verify_header_checksum(&header_bytes)?;
    let tables = read_tables(source, offset, &header, geometry, source_size)?;
    verify_tables_checksum(&tables, header.tables_checksum)?;
    validate_table_layout(&header, tables.len())?;
    build_metadata(
        geometry,
        slot_number,
        source_copy,
        source_size,
        &header,
        &tables,
    )
}

fn build_metadata(
    geometry: LpGeometry,
    slot_number: u32,
    source_copy: MetadataCopy,
    source_size: u64,
    header: &header::MetadataHeader,
    tables: &[u8],
) -> Result<SuperMetadata> {
    validate_entry_size(header.partitions, PARTITION_ENTRY_SIZE, "partition")?;
    validate_entry_size(header.extents, EXTENT_ENTRY_SIZE, "extent")?;
    validate_entry_size(header.groups, GROUP_ENTRY_SIZE, "group")?;
    validate_entry_size(
        header.block_devices,
        BLOCK_DEVICE_ENTRY_SIZE,
        "block-device",
    )?;
    let wire_extents = parse_extents(table_slice(tables, header.extents)?)?;
    let block_devices = parse_block_devices(table_slice(tables, header.block_devices)?)?;
    validate_super_device(geometry, source_size, &block_devices)?;
    let partitions = parse_partitions(
        table_slice(tables, header.partitions)?,
        header,
        slot_number,
        &wire_extents,
        &block_devices,
    )?;
    validate_physical_extents(&partitions)?;
    Ok(SuperMetadata {
        geometry,
        slot_number,
        source_copy,
        minor_version: header.minor_version,
        header_flags: header.flags,
        partitions,
        block_devices,
    })
}
