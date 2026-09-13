use flate2::read::MultiGzDecoder;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub(super) fn skip_exact(reader: &mut dyn Read, mut amount: u64) -> io::Result<()> {
    let mut buffer = [0u8; 32 * 1024];
    while amount > 0 {
        let requested = amount.min(buffer.len() as u64) as usize;
        let read = reader.read(&mut buffer[..requested])?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "archive entry ended before requested offset",
            ));
        }
        amount -= read as u64;
    }
    Ok(())
}

pub(super) struct BoundedFileReader {
    file: File,
    start: u64,
    position: u64,
    length: u64,
}

impl BoundedFileReader {
    pub(super) fn new(mut file: File, length: u64) -> Self {
        let start = file.stream_position().unwrap_or(0);
        Self {
            file,
            start,
            position: 0,
            length,
        }
    }
}

impl Read for BoundedFileReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let remaining = self.length.saturating_sub(self.position);
        if remaining == 0 || buffer.is_empty() {
            return Ok(0);
        }
        let requested = remaining.min(buffer.len() as u64) as usize;
        let read = self.file.read(&mut buffer[..requested])?;
        self.position = self.position.saturating_add(read as u64);
        Ok(read)
    }
}

impl Seek for BoundedFileReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let next = bounded_seek(position, self.position, self.length)?;
        self.position = next;
        self.file
            .seek(SeekFrom::Start(self.start + self.position))?;
        Ok(self.position)
    }
}

pub(super) struct GzipTarEntryReader {
    source_path: PathBuf,
    tar_offset: u64,
    position: u64,
    length: u64,
    decoder: MultiGzDecoder<File>,
}

impl GzipTarEntryReader {
    pub(super) fn new(path: &Path, tar_offset: u64, length: u64) -> io::Result<Self> {
        let mut reader = Self {
            source_path: path.to_path_buf(),
            tar_offset,
            position: 0,
            length,
            decoder: MultiGzDecoder::new(File::open(path)?),
        };
        reader.reset_and_skip(0)?;
        Ok(reader)
    }

    fn reset_and_skip(&mut self, position: u64) -> io::Result<()> {
        self.decoder = MultiGzDecoder::new(File::open(&self.source_path)?);
        skip_exact(&mut self.decoder, self.tar_offset.saturating_add(position))?;
        self.position = position;
        Ok(())
    }
}

impl Read for GzipTarEntryReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let remaining = self.length.saturating_sub(self.position);
        if remaining == 0 || buffer.is_empty() {
            return Ok(0);
        }
        let requested = remaining.min(buffer.len() as u64) as usize;
        let read = self.decoder.read(&mut buffer[..requested])?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "gzip tar entry ended before its declared size",
            ));
        }
        self.position = self.position.saturating_add(read as u64);
        Ok(read)
    }
}

impl Seek for GzipTarEntryReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let next = bounded_seek(position, self.position, self.length)?;
        self.reset_and_skip(next)?;
        Ok(next)
    }
}

pub(super) struct GzipSingleEntryReader {
    source_path: PathBuf,
    position: u64,
    length: u64,
    decoder: MultiGzDecoder<File>,
}

impl GzipSingleEntryReader {
    pub(super) fn new(path: &Path, length: u64) -> io::Result<Self> {
        Ok(Self {
            source_path: path.to_path_buf(),
            position: 0,
            length,
            decoder: MultiGzDecoder::new(File::open(path)?),
        })
    }

    fn reset_and_skip(&mut self, position: u64) -> io::Result<()> {
        self.decoder = MultiGzDecoder::new(File::open(&self.source_path)?);
        skip_exact(&mut self.decoder, position)?;
        self.position = position;
        Ok(())
    }
}

impl Read for GzipSingleEntryReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let remaining = self.length.saturating_sub(self.position);
        if buffer.is_empty() {
            return Ok(0);
        }
        if remaining == 0 {
            let mut probe = [0u8; 1];
            return match self.decoder.read(&mut probe)? {
                0 => Ok(0),
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "gzip payload exceeds its declared size",
                )),
            };
        }
        let requested = remaining.min(buffer.len() as u64) as usize;
        let read = self.decoder.read(&mut buffer[..requested])?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "gzip payload ended before its declared size",
            ));
        }
        self.position = self.position.saturating_add(read as u64);
        Ok(read)
    }
}

impl Seek for GzipSingleEntryReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let next = bounded_seek(position, self.position, self.length)?;
        self.reset_and_skip(next)?;
        Ok(next)
    }
}

fn bounded_seek(position: SeekFrom, current: u64, length: u64) -> io::Result<u64> {
    let next = match position {
        SeekFrom::Start(value) => value as i128,
        SeekFrom::Current(value) => current as i128 + value as i128,
        SeekFrom::End(value) => length as i128 + value as i128,
    };
    if !(0..=length as i128).contains(&next) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "archive seek is outside the entry",
        ));
    }
    Ok(next as u64)
}
