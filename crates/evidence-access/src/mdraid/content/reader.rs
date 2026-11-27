use std::{
    io::{self, Read, Seek, SeekFrom},
    sync::Arc,
};

use evidence_core::{EvidenceReader, ReaderInfo};
use sha2::{Digest, Sha256};

use super::{MdRaidSourceContent, CONTENT_CHUNK_BYTES};

pub(super) struct ContentVerifiedReader {
    source: Box<dyn EvidenceReader>,
    identity: Arc<MdRaidSourceContent>,
    position: u64,
    cached_chunk: Option<u64>,
    bytes: Vec<u8>,
}

impl ContentVerifiedReader {
    pub(super) fn new(
        source: Box<dyn EvidenceReader>,
        identity: Arc<MdRaidSourceContent>,
    ) -> io::Result<Self> {
        if source.info().size != identity.logical_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "source content length changed",
            ));
        }
        Ok(Self {
            source,
            identity,
            position: 0,
            cached_chunk: None,
            bytes: Vec::new(),
        })
    }

    fn load_chunk(&mut self, index: u64) -> io::Result<()> {
        if self.cached_chunk == Some(index) {
            return Ok(());
        }
        self.cached_chunk = None;
        let start = index
            .checked_mul(CONTENT_CHUNK_BYTES as u64)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "content range overflow"))?;
        let count = self
            .identity
            .logical_size
            .saturating_sub(start)
            .min(CONTENT_CHUNK_BYTES as u64) as usize;
        self.bytes.resize(count, 0);
        self.source.seek(SeekFrom::Start(start))?;
        self.source.read_exact(&mut self.bytes)?;
        let digest = hex::encode(Sha256::digest(&self.bytes));
        let expected = usize::try_from(index)
            .ok()
            .and_then(|index| self.identity.chunk_sha256.get(index));
        if expected != Some(&digest) {
            self.bytes.clear();
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "source content digest mismatch",
            ));
        }
        self.cached_chunk = Some(index);
        Ok(())
    }
}

impl Read for ContentVerifiedReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() || self.position >= self.identity.logical_size {
            return Ok(0);
        }
        let chunk = self.position / CONTENT_CHUNK_BYTES as u64;
        self.load_chunk(chunk)?;
        let start = (self.position % CONTENT_CHUNK_BYTES as u64) as usize;
        let count = (self.bytes.len() - start).min(output.len());
        output[..count].copy_from_slice(&self.bytes[start..start + count]);
        self.position += count as u64;
        Ok(count)
    }
}

impl Seek for ContentVerifiedReader {
    fn seek(&mut self, target: SeekFrom) -> io::Result<u64> {
        let position = match target {
            SeekFrom::Start(position) => i128::from(position),
            SeekFrom::Current(delta) => i128::from(self.position) + i128::from(delta),
            SeekFrom::End(delta) => i128::from(self.identity.logical_size) + i128::from(delta),
        };
        self.position = u64::try_from(position).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "content seek exceeds range")
        })?;
        Ok(self.position)
    }
}

impl EvidenceReader for ContentVerifiedReader {
    fn info(&self) -> &ReaderInfo {
        self.source.info()
    }

    fn preferred_read_granularity(&self) -> usize {
        self.source.preferred_read_granularity()
    }
}
