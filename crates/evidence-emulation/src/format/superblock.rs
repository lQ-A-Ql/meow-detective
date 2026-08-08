use std::fs::File;
use std::io::{Seek, SeekFrom, Write};

use crate::crc32c;
use crate::{EmulationError, ParentIdentity};

const MAGIC: &[u8; 8] = b"MDCOW001";
const VERSION: u32 = 2;
const HEADER_SIZE: usize = 4096;
const CHECKSUM_OFFSET: usize = HEADER_SIZE - 4;
pub(crate) const DATA_START: u64 = HEADER_SIZE as u64;

/// The static header written once at overlay creation. It makes the file
/// self-describing (parent length/fingerprint, cluster geometry) for
/// forensic inspection. There is deliberately no generation counter, dual
/// slot or commit protocol: an overlay is a single-session, write-only
/// journal and is never re-opened after the session ends.
#[derive(Debug, Clone)]
pub(crate) struct Superblock {
    pub(crate) parent: ParentIdentity,
    pub(crate) cluster_size: u32,
}

impl Superblock {
    pub(crate) fn new(parent: ParentIdentity, cluster_size: u32) -> Self {
        Self {
            parent,
            cluster_size,
        }
    }

    fn encode(&self) -> [u8; HEADER_SIZE] {
        let mut bytes = [0u8; HEADER_SIZE];
        bytes[..8].copy_from_slice(MAGIC);
        bytes[8..12].copy_from_slice(&VERSION.to_le_bytes());
        bytes[12..16].copy_from_slice(&(HEADER_SIZE as u32).to_le_bytes());
        bytes[24..32].copy_from_slice(&self.parent.logical_length().to_le_bytes());
        bytes[32..36].copy_from_slice(&self.cluster_size.to_le_bytes());
        bytes[40..48].copy_from_slice(&DATA_START.to_le_bytes());
        bytes[64..96].copy_from_slice(self.parent.sha256());
        let checksum = crc32c::checksum(&bytes[..CHECKSUM_OFFSET]);
        bytes[CHECKSUM_OFFSET..].copy_from_slice(&checksum.to_le_bytes());
        bytes
    }
}

pub(crate) fn write_superblocks(
    file: &mut File,
    header: &Superblock,
) -> Result<(), EmulationError> {
    let bytes = header.encode();
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}
