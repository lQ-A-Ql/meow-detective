use crate::BtrfsReader;
use btrfs_driver::fs::NODE_CACHE_BLOCKS;
use evidence_core::EvidenceReader;
use fs_core::{BlockRead, Error as BlockError, Result as BlockResult};
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

const MIN_BTRFS_DEVICE_BYTES: u64 = 0x1_1000;

impl BtrfsReader {
    pub fn open(reader: Box<dyn EvidenceReader>, offset: u64) -> io::Result<Self> {
        let source_size = reader.info().size;
        let device_bytes = source_size.checked_sub(offset).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Btrfs volume offset exceeds evidence length",
            )
        })?;
        if device_bytes < MIN_BTRFS_DEVICE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Btrfs volume is too short to contain its primary superblock",
            ));
        }
        let device = EvidenceBlockDevice {
            reader: Mutex::new(reader),
            offset,
            size: device_bytes,
        };
        let filesystem = btrfs_driver::Filesystem::mount_committed_with_cache(
            Arc::new(device),
            NODE_CACHE_BLOCKS,
        )
        .map_err(btrfs_error)?;
        Ok(Self {
            filesystem: Arc::new(filesystem),
        })
    }

    pub fn has_unreplayed_log(&self) -> bool {
        self.filesystem.superblock().has_dirty_log()
    }
}

struct EvidenceBlockDevice {
    reader: Mutex<Box<dyn EvidenceReader>>,
    offset: u64,
    size: u64,
}

impl BlockRead for EvidenceBlockDevice {
    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> BlockResult<()> {
        let offset = self.offset.checked_add(offset).ok_or_else(|| {
            BlockError::Custom("Btrfs device offset overflowed the evidence reader".into())
        })?;
        let mut reader = self
            .reader
            .lock()
            .map_err(|_| BlockError::Custom("Btrfs evidence reader lock was poisoned".into()))?;
        reader.seek(SeekFrom::Start(offset))?;
        reader.read_exact(buffer)?;
        Ok(())
    }

    fn size_bytes(&self) -> u64 {
        self.size
    }
}

pub(crate) fn btrfs_error(error: btrfs_driver::Error) -> io::Error {
    io::Error::other(error)
}
