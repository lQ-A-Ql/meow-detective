use std::path::Path;
use std::sync::{Condvar, Mutex};

use evidence_core::EvidenceReader;
use image_e01::E01Reader;

use crate::{BlockDeviceError, BlockProvider};

pub(crate) struct E01BlockProvider {
    readers: Mutex<Vec<E01Reader>>,
    available: Condvar,
    byte_len: u64,
}

const READER_POOL_SIZE: usize = 4;

impl E01BlockProvider {
    pub(crate) fn open(path: &Path) -> Result<Self, BlockDeviceError> {
        let reader = E01Reader::open(path)?;
        let byte_len = reader.info().size;
        let mut readers = Vec::with_capacity(READER_POOL_SIZE);
        readers.push(reader);
        for _ in 1..READER_POOL_SIZE {
            let clone = readers[0].try_clone()?;
            readers.push(clone);
        }
        Ok(Self {
            readers: Mutex::new(readers),
            available: Condvar::new(),
            byte_len,
        })
    }
}

impl BlockProvider for E01BlockProvider {
    fn len(&self) -> u64 {
        self.byte_len
    }

    fn read_exact_at(&self, offset: u64, buffer: &mut [u8]) -> Result<(), BlockDeviceError> {
        let mut readers = self
            .readers
            .lock()
            .map_err(|_| BlockDeviceError::LockPoisoned)?;
        let mut reader = loop {
            if let Some(reader) = readers.pop() {
                break reader;
            }
            readers = self
                .available
                .wait(readers)
                .map_err(|_| BlockDeviceError::LockPoisoned)?;
        };
        drop(readers);
        let result = reader
            .read_exact_at(offset, buffer)
            .map_err(BlockDeviceError::from);
        let mut readers = self
            .readers
            .lock()
            .map_err(|_| BlockDeviceError::LockPoisoned)?;
        readers.push(reader);
        self.available.notify_one();
        result
    }
}
