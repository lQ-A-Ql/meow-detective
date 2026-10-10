use crate::reader::{EvidenceReader, ReaderInfo};
mod formats;
use formats::{read_backend, Backend};
mod open;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;

/// Read-only reader for raw/dd images and supported monolithic-flat VMDKs.
///
/// VMDK support intentionally covers descriptor files which reference one
/// zero-offset `FLAT` extent (`createType="monolithicFlat"`). Sparse,
/// compressed, snapshot and parent-chain descriptors are rejected rather than
/// guessed. The descriptor path remains the provenance path in `ReaderInfo`.
#[derive(Debug)]
pub struct RawImageReader {
    backend: Backend,
    info: ReaderInfo,
    cursor: u64,
    backing_paths: Vec<PathBuf>,
}

impl Read for RawImageReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.cursor >= self.info.size || buf.is_empty() {
            return Ok(0);
        }
        let available = self.info.size - self.cursor;
        let requested = available.min(buf.len() as u64) as usize;
        let read = read_backend(&mut self.backend, self.cursor, &mut buf[..requested])?;
        self.cursor = self.cursor.saturating_add(read as u64);
        Ok(read)
    }
}

impl Seek for RawImageReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let next = match pos {
            SeekFrom::Start(offset) => i128::from(offset),
            SeekFrom::Current(offset) => i128::from(self.cursor) + i128::from(offset),
            SeekFrom::End(offset) => i128::from(self.info.size) + i128::from(offset),
        };
        if next < 0 || next > i128::from(u64::MAX) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek position is outside the addressable image range",
            ));
        }
        self.cursor = next as u64;
        Ok(self.cursor)
    }
}

impl EvidenceReader for RawImageReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/image/raw_reader.rs"]
mod tests;
