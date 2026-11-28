use std::io::{self, Read, Seek, SeekFrom};

use evidence_core::{EvidenceReader, ReaderInfo};

use super::PreparedCephFsFileReader;

pub struct CephFsEvidenceReader {
    reader: PreparedCephFsFileReader,
    info: ReaderInfo,
    position: u64,
}

impl CephFsEvidenceReader {
    pub fn new(reader: PreparedCephFsFileReader, size: u64) -> Self {
        Self {
            reader,
            info: ReaderInfo {
                path: "cephfs-file".into(),
                size,
                kind: "ceph_fs".into(),
            },
            position: 0,
        }
    }
}

impl Read for CephFsEvidenceReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let bytes = self
            .reader
            .read_range(self.position, output.len())
            .map_err(io::Error::other)?;
        output[..bytes.len()].copy_from_slice(&bytes);
        self.position += bytes.len() as u64;
        Ok(bytes.len())
    }
}

impl Seek for CephFsEvidenceReader {
    fn seek(&mut self, target: SeekFrom) -> io::Result<u64> {
        self.reader.validate_scope().map_err(io::Error::other)?;
        let position = match target {
            SeekFrom::Start(position) => Some(position),
            SeekFrom::Current(delta) => self.position.checked_add_signed(delta),
            SeekFrom::End(delta) => self.info.size.checked_add_signed(delta),
        }
        .filter(|position| *position <= self.info.size)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "CephFS file seek exceeds its bounds",
            )
        })?;
        self.position = position;
        self.reader.validate_scope().map_err(io::Error::other)?;
        Ok(position)
    }
}
impl EvidenceReader for CephFsEvidenceReader {
    fn validate_read_scope(&self) -> io::Result<()> {
        self.reader.validate_scope().map_err(io::Error::other)
    }
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}
