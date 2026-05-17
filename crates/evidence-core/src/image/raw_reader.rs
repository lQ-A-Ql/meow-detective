use crate::reader::{EvidenceReader, ReaderInfo};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

pub struct RawImageReader {
    file: File,
    info: ReaderInfo,
}

impl RawImageReader {
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let metadata = file.metadata()?;
        Ok(Self {
            info: ReaderInfo {
                path: path.to_path_buf(),
                size: metadata.len(),
                kind: "raw".to_string(),
            },
            file,
        })
    }
}

impl Read for RawImageReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read(buf)
    }
}

impl Seek for RawImageReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.file.seek(pos)
    }
}

impl EvidenceReader for RawImageReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}
