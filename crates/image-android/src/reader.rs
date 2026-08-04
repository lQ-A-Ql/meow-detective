use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use evidence_core::{EvidenceReader, ReaderInfo};

use crate::error::{Result, SparseImageError};
use crate::format::{SparseChunkKind, SparseImage};

pub struct AndroidSparseReader {
    file: File,
    image: SparseImage,
    cursor: u64,
    info: ReaderInfo,
}

impl AndroidSparseReader {
    pub fn open(path: &Path) -> Result<Self> {
        let mut file = File::open(path)?;
        let image = SparseImage::parse(&mut file)?;
        let info = ReaderInfo {
            path: path.to_path_buf(),
            size: image.logical_size(),
            kind: "android-sparse".to_string(),
        };
        Ok(Self {
            file,
            image,
            cursor: 0,
            info,
        })
    }

    pub fn image(&self) -> &SparseImage {
        &self.image
    }

    pub fn logical_size(&self) -> u64 {
        self.image.logical_size()
    }

    pub fn read_range(&mut self, offset: u64, buffer: &mut [u8]) -> Result<usize> {
        if offset >= self.logical_size() || buffer.is_empty() {
            return Ok(0);
        }
        let remaining = usize::try_from(self.logical_size() - offset).unwrap_or(usize::MAX);
        let requested = buffer.len().min(remaining);
        let mut total = 0usize;
        let mut position = offset;
        while total < requested {
            let chunk = self
                .image
                .chunk_for(position)
                .ok_or(SparseImageError::MissingChunk(position))?;
            let intra = position - chunk.logical_offset;
            let available = usize::try_from(chunk.logical_length - intra).unwrap_or(usize::MAX);
            let length = available.min(requested - total);
            match chunk.kind {
                SparseChunkKind::Raw => {
                    let source_offset = chunk
                        .source_offset
                        .checked_add(intra)
                        .ok_or(SparseImageError::ArithmeticOverflow("raw source offset"))?;
                    self.file.seek(SeekFrom::Start(source_offset))?;
                    self.file.read_exact(&mut buffer[total..total + length])?;
                }
                SparseChunkKind::Fill(pattern) => {
                    fill_pattern(&mut buffer[total..total + length], &pattern, intra);
                }
                SparseChunkKind::DontCare => buffer[total..total + length].fill(0),
            }
            total += length;
            position += length as u64;
        }
        Ok(total)
    }

    pub fn try_clone(&self) -> Result<Self> {
        Ok(Self {
            file: self.file.try_clone()?,
            image: self.image.clone(),
            cursor: 0,
            info: self.info.clone(),
        })
    }
}

impl Read for AndroidSparseReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let read = self
            .read_range(self.cursor, buffer)
            .map_err(io::Error::other)?;
        self.cursor = self.cursor.saturating_add(read as u64);
        Ok(read)
    }
}

impl Seek for AndroidSparseReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let next = match position {
            SeekFrom::Start(value) => i128::from(value),
            SeekFrom::End(value) => i128::from(self.logical_size()) + i128::from(value),
            SeekFrom::Current(value) => i128::from(self.cursor) + i128::from(value),
        };
        if next < 0 || next > i128::from(u64::MAX) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "sparse reader seek is outside the addressable range",
            ));
        }
        self.cursor = next as u64;
        Ok(self.cursor)
    }
}

impl EvidenceReader for AndroidSparseReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

fn fill_pattern(buffer: &mut [u8], pattern: &[u8; 4], offset: u64) {
    let start = (offset % 4) as usize;
    for (index, byte) in buffer.iter_mut().enumerate() {
        *byte = pattern[(start + index) % 4];
    }
}
