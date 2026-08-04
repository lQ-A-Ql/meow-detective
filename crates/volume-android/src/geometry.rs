use std::io::{Read, Seek, SeekFrom};

use sha2::{Digest, Sha256};

use crate::bytes::{read_checksum, read_u32};
use crate::error::{Result, VolumeAndroidError};
use crate::{LP_METADATA_GEOMETRY_MAGIC, LP_SECTOR_SIZE};

pub(crate) const LP_PARTITION_RESERVED_BYTES: u64 = 4096;
pub(crate) const LP_METADATA_GEOMETRY_SIZE: u64 = 4096;
const LP_GEOMETRY_STRUCT_SIZE: usize = 52;
const PRIMARY_GEOMETRY_OFFSET: u64 = LP_PARTITION_RESERVED_BYTES;
const BACKUP_GEOMETRY_OFFSET: u64 = PRIMARY_GEOMETRY_OFFSET + LP_METADATA_GEOMETRY_SIZE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometryCopy {
    Primary,
    Backup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LpGeometry {
    pub metadata_max_size: u32,
    pub metadata_slot_count: u32,
    pub logical_block_size: u32,
    pub source_copy: GeometryCopy,
}

impl LpGeometry {
    pub fn read<R: Read + Seek>(source: &mut R) -> Result<Self> {
        match read_copy(source, PRIMARY_GEOMETRY_OFFSET, GeometryCopy::Primary) {
            Ok(geometry) => Ok(geometry),
            Err(primary) => match read_copy(source, BACKUP_GEOMETRY_OFFSET, GeometryCopy::Backup) {
                Ok(geometry) => Ok(geometry),
                Err(backup) => Err(VolumeAndroidError::GeometryCopiesInvalid {
                    primary: primary.to_string(),
                    backup: backup.to_string(),
                }),
            },
        }
    }

    pub(crate) fn primary_metadata_offset(self, slot: u32) -> Result<u64> {
        metadata_base_offset()?
            .checked_add(slot_offset(self, slot)?)
            .ok_or(VolumeAndroidError::ArithmeticOverflow(
                "primary metadata offset",
            ))
    }

    pub(crate) fn backup_metadata_offset(self, slot: u32) -> Result<u64> {
        let selected_slot = slot_offset(self, slot)?;
        let primary_region = u64::from(self.metadata_max_size)
            .checked_mul(u64::from(self.metadata_slot_count))
            .ok_or(VolumeAndroidError::ArithmeticOverflow(
                "primary metadata region",
            ))?;
        metadata_base_offset()?
            .checked_add(primary_region)
            .and_then(|offset| offset.checked_add(selected_slot))
            .ok_or(VolumeAndroidError::ArithmeticOverflow(
                "backup metadata offset",
            ))
    }

    pub(crate) fn total_metadata_size(self) -> Result<u64> {
        let slots = u64::from(self.metadata_max_size)
            .checked_mul(u64::from(self.metadata_slot_count))
            .ok_or(VolumeAndroidError::ArithmeticOverflow("metadata slots"))?;
        LP_METADATA_GEOMETRY_SIZE
            .checked_add(slots)
            .and_then(|side| side.checked_mul(2))
            .and_then(|size| size.checked_add(LP_PARTITION_RESERVED_BYTES))
            .ok_or(VolumeAndroidError::ArithmeticOverflow(
                "total metadata size",
            ))
    }
}

fn read_copy<R: Read + Seek>(
    source: &mut R,
    offset: u64,
    copy: GeometryCopy,
) -> Result<LpGeometry> {
    let mut bytes = [0u8; LP_GEOMETRY_STRUCT_SIZE];
    source.seek(SeekFrom::Start(offset))?;
    source.read_exact(&mut bytes)?;
    if read_u32(&bytes, 0, "geometry magic")? != LP_METADATA_GEOMETRY_MAGIC {
        return Err(VolumeAndroidError::InvalidGeometry(
            "magic does not match liblp geometry".to_string(),
        ));
    }
    if read_u32(&bytes, 4, "geometry struct size")? as usize != LP_GEOMETRY_STRUCT_SIZE {
        return Err(VolumeAndroidError::InvalidGeometry(
            "unsupported geometry struct size".to_string(),
        ));
    }
    verify_geometry_checksum(&bytes)?;
    let geometry = LpGeometry {
        metadata_max_size: read_u32(&bytes, 40, "metadata max size")?,
        metadata_slot_count: read_u32(&bytes, 44, "metadata slot count")?,
        logical_block_size: read_u32(&bytes, 48, "logical block size")?,
        source_copy: copy,
    };
    validate_geometry(geometry)?;
    Ok(geometry)
}

fn verify_geometry_checksum(bytes: &[u8; LP_GEOMETRY_STRUCT_SIZE]) -> Result<()> {
    let expected = read_checksum(bytes, 8)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes[..8]);
    hasher.update([0u8; 32]);
    hasher.update(&bytes[40..]);
    if hasher.finalize().as_slice() != expected {
        return Err(VolumeAndroidError::InvalidGeometry(
            "SHA-256 checksum mismatch".to_string(),
        ));
    }
    Ok(())
}

fn validate_geometry(geometry: LpGeometry) -> Result<()> {
    if geometry.metadata_max_size == 0
        || !u64::from(geometry.metadata_max_size).is_multiple_of(LP_SECTOR_SIZE)
    {
        return Err(VolumeAndroidError::InvalidGeometry(
            "metadata max size must be non-zero and sector aligned".to_string(),
        ));
    }
    if geometry.metadata_slot_count == 0 {
        return Err(VolumeAndroidError::InvalidGeometry(
            "metadata slot count must be non-zero".to_string(),
        ));
    }
    if geometry.logical_block_size == 0
        || !u64::from(geometry.logical_block_size).is_multiple_of(LP_SECTOR_SIZE)
    {
        return Err(VolumeAndroidError::InvalidGeometry(
            "logical block size must be non-zero and sector aligned".to_string(),
        ));
    }
    Ok(())
}

fn metadata_base_offset() -> Result<u64> {
    LP_METADATA_GEOMETRY_SIZE
        .checked_mul(2)
        .and_then(|size| size.checked_add(LP_PARTITION_RESERVED_BYTES))
        .ok_or(VolumeAndroidError::ArithmeticOverflow("metadata base"))
}

fn slot_offset(geometry: LpGeometry, slot: u32) -> Result<u64> {
    if slot >= geometry.metadata_slot_count {
        return Err(VolumeAndroidError::InvalidGeometry(format!(
            "metadata slot {slot} is outside {} slots",
            geometry.metadata_slot_count
        )));
    }
    u64::from(geometry.metadata_max_size)
        .checked_mul(u64::from(slot))
        .ok_or(VolumeAndroidError::ArithmeticOverflow(
            "metadata slot offset",
        ))
}
