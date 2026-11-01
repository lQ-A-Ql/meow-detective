use std::io::SeekFrom;
use std::sync::{Arc, Mutex};

use evidence_core::EvidenceReader;

use crate::{BlockDeviceError, BlockProvider};

/// Adapts a source-bound sequential evidence reader to the random-access block
/// provider used by overlays and emulation. The reader is serialized because
/// `EvidenceReader` exposes a mutable seek cursor; the evidence remains
/// read-only and every request is bounded before touching that cursor.
pub struct EvidenceReaderBlockProvider<R: EvidenceReader + 'static> {
    reader: Arc<Mutex<R>>,
    byte_len: u64,
}

impl<R: EvidenceReader + 'static> EvidenceReaderBlockProvider<R> {
    pub fn new(reader: R) -> Self {
        let byte_len = reader.info().size;
        Self {
            reader: Arc::new(Mutex::new(reader)),
            byte_len,
        }
    }

    pub fn shared(reader: Arc<Mutex<R>>) -> Result<Self, BlockDeviceError> {
        let byte_len = reader
            .lock()
            .map_err(|_| BlockDeviceError::LockPoisoned)?
            .info()
            .size;
        Ok(Self { reader, byte_len })
    }
}

impl<R: EvidenceReader + 'static> BlockProvider for EvidenceReaderBlockProvider<R> {
    fn len(&self) -> u64 {
        self.byte_len
    }

    fn read_exact_at(&self, offset: u64, buffer: &mut [u8]) -> Result<(), BlockDeviceError> {
        let request_len =
            u64::try_from(buffer.len()).map_err(|_| BlockDeviceError::ArithmeticOverflow)?;
        let end = offset
            .checked_add(request_len)
            .ok_or(BlockDeviceError::ArithmeticOverflow)?;
        if end > self.byte_len {
            return Err(BlockDeviceError::OutOfBounds {
                offset,
                end,
                size: self.byte_len,
            });
        }
        let mut reader = self
            .reader
            .lock()
            .map_err(|_| BlockDeviceError::LockPoisoned)?;
        reader.seek(SeekFrom::Start(offset))?;
        reader.read_exact(buffer)?;
        Ok(())
    }
}
