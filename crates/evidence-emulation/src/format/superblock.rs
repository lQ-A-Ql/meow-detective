use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

use uuid::Uuid;

use crate::crc32c;
use crate::{EmulationError, ParentIdentity};

const MAGIC: &[u8; 8] = b"MDCOW001";
const VERSION: u32 = 1;
const SUPERBLOCK_SIZE: usize = 4096;
const SUPERBLOCK_COUNT: u64 = 2;
const CHECKSUM_OFFSET: usize = SUPERBLOCK_SIZE - 4;
pub(crate) const DATA_START: u64 = SUPERBLOCK_SIZE as u64 * SUPERBLOCK_COUNT;

#[derive(Debug, Clone)]
pub(crate) struct Superblock {
    pub(crate) generation: u64,
    pub(crate) parent: ParentIdentity,
    pub(crate) cluster_size: u32,
    pub(crate) overlay_id: Uuid,
}

impl Superblock {
    pub(crate) fn new(parent: ParentIdentity, cluster_size: u32) -> Self {
        Self {
            generation: 0,
            parent,
            cluster_size,
            overlay_id: Uuid::new_v4(),
        }
    }

    fn encode(&self) -> [u8; SUPERBLOCK_SIZE] {
        let mut bytes = [0u8; SUPERBLOCK_SIZE];
        bytes[..8].copy_from_slice(MAGIC);
        bytes[8..12].copy_from_slice(&VERSION.to_le_bytes());
        bytes[12..16].copy_from_slice(&(SUPERBLOCK_SIZE as u32).to_le_bytes());
        bytes[16..24].copy_from_slice(&self.generation.to_le_bytes());
        bytes[24..32].copy_from_slice(&self.parent.logical_length().to_le_bytes());
        bytes[32..36].copy_from_slice(&self.cluster_size.to_le_bytes());
        bytes[40..48].copy_from_slice(&DATA_START.to_le_bytes());
        bytes[48..64].copy_from_slice(self.overlay_id.as_bytes());
        bytes[64..96].copy_from_slice(self.parent.sha256());
        let checksum = crc32c::checksum(&bytes[..CHECKSUM_OFFSET]);
        bytes[CHECKSUM_OFFSET..].copy_from_slice(&checksum.to_le_bytes());
        bytes
    }

    fn decode(bytes: &[u8; SUPERBLOCK_SIZE]) -> Result<Self, EmulationError> {
        if &bytes[..8] != MAGIC || u32_at(bytes, 8) != VERSION {
            return Err(corrupt("invalid superblock magic or version"));
        }
        if u32_at(bytes, 12) != SUPERBLOCK_SIZE as u32 || u64_at(bytes, 40) != DATA_START {
            return Err(corrupt("invalid superblock geometry"));
        }
        let expected = u32_at(bytes, CHECKSUM_OFFSET);
        if crc32c::checksum(&bytes[..CHECKSUM_OFFSET]) != expected {
            return Err(corrupt("superblock checksum mismatch"));
        }
        let parent_hash = bytes[64..96]
            .try_into()
            .map_err(|_| corrupt("invalid parent hash"))?;
        let overlay_id =
            Uuid::from_slice(&bytes[48..64]).map_err(|_| corrupt("invalid overlay identifier"))?;
        Ok(Self {
            generation: u64_at(bytes, 16),
            parent: ParentIdentity::new(u64_at(bytes, 24), parent_hash)?,
            cluster_size: u32_at(bytes, 32),
            overlay_id,
        })
    }
}

pub(crate) fn write_superblocks(
    file: &mut File,
    header: &Superblock,
) -> Result<(), EmulationError> {
    let bytes = header.encode();
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&bytes)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

pub(crate) fn write_superblock_slot(
    file: &mut File,
    header: &Superblock,
) -> Result<(), EmulationError> {
    let slot = header.generation % SUPERBLOCK_COUNT;
    file.seek(SeekFrom::Start(slot * SUPERBLOCK_SIZE as u64))?;
    file.write_all(&header.encode())?;
    Ok(())
}

pub(crate) fn read_superblock(file: &mut File) -> Result<Superblock, EmulationError> {
    let mut valid = Vec::new();
    for slot in 0..SUPERBLOCK_COUNT {
        let mut bytes = [0u8; SUPERBLOCK_SIZE];
        file.seek(SeekFrom::Start(slot * SUPERBLOCK_SIZE as u64))?;
        if file.read_exact(&mut bytes).is_ok() {
            if let Ok(header) = Superblock::decode(&bytes) {
                valid.push(header);
            }
        }
    }
    valid
        .into_iter()
        .max_by_key(|header| header.generation)
        .ok_or_else(|| corrupt("both superblock copies are invalid"))
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap_or([0; 4]))
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap_or([0; 8]))
}

fn corrupt(message: &str) -> EmulationError {
    EmulationError::CorruptOverlay(message.to_string())
}
