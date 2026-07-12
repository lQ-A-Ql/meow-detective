use evidence_core::{EvidenceReader, ReaderInfo};
use std::io::{self, Read, Seek, SeekFrom};

pub(crate) struct FakeReader {
    pub(crate) data: Vec<u8>,
    pos: u64,
    info: ReaderInfo,
}

impl FakeReader {
    pub(crate) fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            pos: 0,
            info: ReaderInfo {
                path: std::path::PathBuf::from("fake-btrfs"),
                size: 0,
                kind: "fake-btrfs".to_string(),
            },
        }
    }
}

impl Read for FakeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let start = (self.pos as usize).min(self.data.len());
        let end = (start + buf.len()).min(self.data.len());
        let n = end - start;
        buf[..n].copy_from_slice(&self.data[start..end]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for FakeReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.pos = match pos {
            SeekFrom::Start(position) => position,
            SeekFrom::End(delta) => (self.data.len() as i64 + delta).max(0) as u64,
            SeekFrom::Current(delta) => (self.pos as i64 + delta).max(0) as u64,
        };
        Ok(self.pos)
    }
}

impl EvidenceReader for FakeReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}
