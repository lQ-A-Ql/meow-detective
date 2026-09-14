use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

pub(crate) struct BoundedFileReader {
    file: File,
    start: u64,
    position: u64,
    length: u64,
}

impl BoundedFileReader {
    pub(crate) fn new(mut file: File, length: u64) -> Self {
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
