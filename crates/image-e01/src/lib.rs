//! E01 (EWF) image reader stub.
//! Provides EvidenceReader trait impl; full EWF format parsing is future work.

use evidence_core::{EvidenceReader, ReaderInfo};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

pub struct E01Reader {
    info: ReaderInfo,
}

impl E01Reader {
    pub fn open(path: &Path) -> io::Result<Self> {
        let metadata = std::fs::metadata(path)?;
        Ok(Self {
            info: ReaderInfo { path: path.to_path_buf(), size: metadata.len(), kind: "e01".into() },
        })
    }
}

impl Read for E01Reader {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "E01 reader not yet implemented"))
    }
}

impl Seek for E01Reader {
    fn seek(&mut self, _pos: SeekFrom) -> io::Result<u64> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "E01 reader not yet implemented"))
    }
}

impl EvidenceReader for E01Reader {
    fn info(&self) -> &ReaderInfo { &self.info }
}
