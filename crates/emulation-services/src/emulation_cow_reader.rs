//! Read-only `EvidenceReader` view over a session `CowDisk`.
//!
//! The overlay disk is the writable side of an emulation session; this
//! adapter exposes its read path through the standard reader trait so a
//! filesystem parser can re-open the edited volume for post-write
//! verification (e.g. confirming a removed directory entry is gone). Reads
//! observe overlay writes exactly as the guest would.

use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;

use evidence_core::{EvidenceReader, ReaderInfo};
use evidence_emulation::CowDisk;

pub struct CowDiskReader {
    disk: Arc<CowDisk>,
    position: u64,
    info: ReaderInfo,
}

impl CowDiskReader {
    pub fn new(disk: Arc<CowDisk>) -> Self {
        let info = ReaderInfo {
            path: std::path::PathBuf::from("<emulation-cow-overlay>"),
            size: disk.len(),
            kind: "emulation-cow".to_string(),
        };
        Self {
            disk,
            position: 0,
            info,
        }
    }
}

impl Read for CowDiskReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.position >= self.info.size {
            return Ok(0);
        }
        let wanted = (self.info.size - self.position).min(buffer.len() as u64) as usize;
        self.disk
            .read_exact_at(self.position, &mut buffer[..wanted])
            .map_err(std::io::Error::other)?;
        self.position += wanted as u64;
        Ok(wanted)
    }
}

impl Seek for CowDiskReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let next = match position {
            SeekFrom::Start(offset) => offset as i128,
            SeekFrom::Current(delta) => self.position as i128 + delta as i128,
            SeekFrom::End(delta) => self.info.size as i128 + delta as i128,
        };
        if next < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek before the start of the disk",
            ));
        }
        self.position = next as u64;
        Ok(self.position)
    }
}

impl EvidenceReader for CowDiskReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}
