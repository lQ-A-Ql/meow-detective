use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

use evidence_core::{EvidenceReader, ReaderInfo};

use super::super::rados_reader::{RadosObjectLayout, RadosObjectReader};

#[derive(Clone)]
pub(super) struct SharedEvidenceReader {
    inner: Arc<Mutex<Box<dyn EvidenceReader>>>,
    info: ReaderInfo,
}

impl SharedEvidenceReader {
    pub(super) fn new(reader: Box<dyn EvidenceReader>) -> Self {
        let info = reader.info().clone();
        Self {
            inner: Arc::new(Mutex::new(reader)),
            info,
        }
    }

    fn lock(&self) -> std::io::Result<std::sync::MutexGuard<'_, Box<dyn EvidenceReader>>> {
        self.inner
            .lock()
            .map_err(|_| invalid_data_error("shared evidence reader lock is poisoned".to_string()))
    }
}

impl Read for SharedEvidenceReader {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        self.lock()?.read(output)
    }
}

impl Seek for SharedEvidenceReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.lock()?.seek(position)
    }
}

impl EvidenceReader for SharedEvidenceReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

pub(super) fn read_plan_page(
    device: SharedEvidenceReader,
    plan: Arc<RadosObjectLayout>,
    offset: u64,
    length: usize,
) -> std::io::Result<Vec<u8>> {
    let mut reader = RadosObjectReader::from_layout(Box::new(device), plan);
    reader.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::with_capacity(length);
    reader.take(length as u64).read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn invalid_data_error(message: String) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message)
}
