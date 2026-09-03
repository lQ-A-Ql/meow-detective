use evidence_core::{EvidenceReader, RawImageReader};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;

use crate::{BlockDeviceError, BlockProvider};

pub(crate) struct RawBlockProvider {
    file: Mutex<RawImageReader>,
    byte_len: u64,
}

impl RawBlockProvider {
    pub(crate) fn open(path: &Path) -> Result<Self, BlockDeviceError> {
        let reader = RawImageReader::open(path)?;
        let byte_len = reader.info().size;
        Ok(Self {
            file: Mutex::new(reader),
            byte_len,
        })
    }
}

impl BlockProvider for RawBlockProvider {
    fn len(&self) -> u64 {
        self.byte_len
    }

    fn read_exact_at(&self, offset: u64, buffer: &mut [u8]) -> Result<(), BlockDeviceError> {
        let mut file = self
            .file
            .lock()
            .map_err(|_| BlockDeviceError::LockPoisoned)?;
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(buffer)?;
        Ok(())
    }
}
