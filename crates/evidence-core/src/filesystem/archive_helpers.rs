use flate2::read::MultiGzDecoder;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use super::archive_index::MAX_ENTRY_SIZE;

pub(super) fn gzip_stream_looks_like_tar(path: &Path) -> io::Result<bool> {
    let file = File::open(path)?;
    let mut decoder = MultiGzDecoder::new(file);
    let mut header = [0u8; 512];
    let count = decoder.read(&mut header)?;
    if count < 265 {
        return Ok(false);
    }
    if &header[257..262] == b"ustar" {
        return Ok(true);
    }
    Ok(tar_checksum_valid(&header))
}

fn tar_checksum_valid(header: &[u8; 512]) -> bool {
    let stored = std::str::from_utf8(&header[148..156])
        .ok()
        .and_then(|value| u64::from_str_radix(value.trim_matches('\0').trim(), 8).ok());
    let Some(stored) = stored else {
        return false;
    };
    let sum = header.iter().enumerate().fold(0u64, |sum, (index, byte)| {
        sum + if (148..156).contains(&index) {
            b' ' as u64
        } else {
            *byte as u64
        }
    });
    sum == stored
}

pub(super) fn gzip_size(path: &Path) -> io::Result<u64> {
    let file = File::open(path)?;
    let mut decoder = InflatedLimit::new(MultiGzDecoder::new(file), MAX_ENTRY_SIZE);
    let mut buffer = [0u8; 32 * 1024];
    let mut total = 0u64;
    loop {
        let read = decoder
            .read(&mut buffer)
            .map_err(|error| invalid_data(format!("invalid gzip payload: {error}")))?;
        if read == 0 {
            return Ok(total);
        }
        total = total.saturating_add(read as u64);
    }
}

pub(super) struct InflatedLimit<R> {
    inner: R,
    total: u64,
    limit: u64,
}

impl<R> InflatedLimit<R> {
    pub(super) fn new(inner: R, limit: u64) -> Self {
        Self {
            inner,
            total: 0,
            limit,
        }
    }
}

impl<R: Read> Read for InflatedLimit<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.total >= self.limit {
            let mut probe = [0u8; 1];
            return match self.inner.read(&mut probe)? {
                0 => Ok(0),
                _ => Err(invalid_data(
                    "archive decompressed size exceeds the global limit",
                )),
            };
        }
        let remaining = self.limit - self.total;
        let requested = remaining.min(buffer.len() as u64) as usize;
        let read = self.inner.read(&mut buffer[..requested])?;
        self.total = self.total.saturating_add(read as u64);
        Ok(read)
    }
}

pub(super) fn strip_gzip_suffix(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".gzip") {
        name[..name.len() - 5].to_string()
    } else if lower.ends_with(".gz") {
        name[..name.len() - 3].to_string()
    } else {
        format!("{name}.decompressed")
    }
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
#[path = "../../tests/unit/filesystem/archive_helpers.rs"]
mod tests;
