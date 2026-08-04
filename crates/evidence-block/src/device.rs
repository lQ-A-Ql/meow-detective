use std::path::Path;
use std::sync::Arc;

use iscsi_target::{IscsiError, ScsiBlockDevice, ScsiResult};

use crate::{
    open_block_provider, BlockDeviceError, BlockGeometry, BlockProvider, EvidenceImageKind,
};

const DEFAULT_BLOCK_SIZE: u32 = 512;
const MAX_SCSI_READ_BYTES: usize = 16 * 1024 * 1024;

pub struct ReadOnlyScsiDevice {
    provider: Arc<dyn BlockProvider>,
    geometry: BlockGeometry,
}

impl ReadOnlyScsiDevice {
    pub fn open(path: &Path, kind: EvidenceImageKind) -> Result<Self, BlockDeviceError> {
        Self::from_provider(open_block_provider(path, kind)?)
    }

    pub fn from_provider(provider: Arc<dyn BlockProvider>) -> Result<Self, BlockDeviceError> {
        let geometry = BlockGeometry::new(provider.len(), DEFAULT_BLOCK_SIZE)?;
        Ok(Self { provider, geometry })
    }

    pub fn geometry(&self) -> BlockGeometry {
        self.geometry
    }

    fn read_blocks(&self, lba: u64, blocks: u32) -> Result<Vec<u8>, BlockDeviceError> {
        let (offset, length) = self.geometry.byte_range(lba, blocks)?;
        if length > MAX_SCSI_READ_BYTES as u64 {
            return Err(BlockDeviceError::RequestTooLarge {
                requested: length,
                maximum: MAX_SCSI_READ_BYTES,
            });
        }
        let buffer_len =
            usize::try_from(length).map_err(|_| BlockDeviceError::ArithmeticOverflow)?;
        let mut buffer = vec![0u8; buffer_len];
        self.provider.read_exact_at(offset, &mut buffer)?;
        Ok(buffer)
    }
}

impl ScsiBlockDevice for ReadOnlyScsiDevice {
    fn read(&self, lba: u64, blocks: u32, block_size: u32) -> ScsiResult<Vec<u8>> {
        if block_size != self.geometry.block_size() {
            return Err(IscsiError::Scsi(format!(
                "unexpected block size {block_size}; expected {}",
                self.geometry.block_size()
            )));
        }
        self.read_blocks(lba, blocks)
            .map_err(|error| IscsiError::Scsi(error.to_string()))
    }

    fn write(&mut self, _lba: u64, _data: &[u8], _block_size: u32) -> ScsiResult<()> {
        Err(IscsiError::Scsi("device is write protected".to_string()))
    }

    fn capacity(&self) -> u64 {
        self.geometry.block_count()
    }

    fn block_size(&self) -> u32 {
        self.geometry.block_size()
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn vendor_id(&self) -> &str {
        "MEOWDET"
    }

    fn product_id(&self) -> &str {
        "FORENSIC DISK"
    }

    fn product_rev(&self) -> &str {
        "1.0"
    }
}
