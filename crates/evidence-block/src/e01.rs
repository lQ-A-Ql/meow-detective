use std::path::Path;
use std::sync::Mutex;

use evidence_core::EvidenceReader;
use image_e01::E01Reader;

use crate::{BlockDeviceError, BlockProvider};

pub(crate) struct E01BlockProvider {
    reader: Mutex<E01Reader>,
    byte_len: u64,
}

impl E01BlockProvider {
    pub(crate) fn open(path: &Path) -> Result<Self, BlockDeviceError> {
        let reader = E01Reader::open(path)?;
        let byte_len = reader.info().size;
        Ok(Self {
            reader: Mutex::new(reader),
            byte_len,
        })
    }
}

impl BlockProvider for E01BlockProvider {
    fn len(&self) -> u64 {
        self.byte_len
    }

    fn read_exact_at(&self, offset: u64, buffer: &mut [u8]) -> Result<(), BlockDeviceError> {
        let mut reader = self
            .reader
            .lock()
            .map_err(|_| BlockDeviceError::LockPoisoned)?;
        reader.read_exact_at(offset, buffer)?;
        Ok(())
    }
}
