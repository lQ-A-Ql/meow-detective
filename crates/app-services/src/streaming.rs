//! Streaming file processing.
//!
//! Provides streaming processing capabilities for large files,
//! reducing memory usage and improving performance.

use std::io::{self, Read};

/// Streaming processor trait
pub trait StreamingProcessor {
    /// Process a chunk of data
    fn process_chunk(&mut self, data: &[u8]) -> io::Result<()>;

    /// Finalize processing and return result
    fn finalize(&mut self) -> io::Result<StreamingResult>;
}

/// Result from streaming processing
#[derive(Debug, Clone)]
pub struct StreamingResult {
    /// Total bytes processed
    pub bytes_processed: u64,
    /// Processing time in milliseconds
    pub processing_time_ms: u64,
    /// Any warnings
    pub warnings: Vec<String>,
}

/// Streaming hasher for computing hash while reading
pub struct StreamingHasher {
    hasher: sha2::Sha256,
    bytes_processed: u64,
}

impl StreamingHasher {
    /// Create a new streaming hasher
    pub fn new() -> Self {
        use sha2::Digest;
        Self {
            hasher: sha2::Sha256::new(),
            bytes_processed: 0,
        }
    }
}

impl Default for StreamingHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingProcessor for StreamingHasher {
    fn process_chunk(&mut self, data: &[u8]) -> io::Result<()> {
        use sha2::Digest;
        self.hasher.update(data);
        self.bytes_processed += data.len() as u64;
        Ok(())
    }

    fn finalize(&mut self) -> io::Result<StreamingResult> {
        use sha2::Digest;
        let _hash = self.hasher.finalize_reset();
        Ok(StreamingResult {
            bytes_processed: self.bytes_processed,
            processing_time_ms: 0,
            warnings: Vec::new(),
        })
    }
}

/// Streaming file reader with progress tracking
pub struct StreamingReader<R: Read> {
    reader: R,
    buffer: Vec<u8>,
    bytes_read: u64,
    total_bytes: u64,
}

impl<R: Read> StreamingReader<R> {
    /// Create a new streaming reader
    pub fn new(reader: R, total_bytes: u64, buffer_size: usize) -> Self {
        Self {
            reader,
            buffer: vec![0u8; buffer_size],
            bytes_read: 0,
            total_bytes,
        }
    }

    /// Read next chunk, returns bytes read
    pub fn read_chunk(&mut self) -> io::Result<&[u8]> {
        let bytes_read = self.reader.read(&mut self.buffer)?;
        self.bytes_read += bytes_read as u64;
        Ok(&self.buffer[..bytes_read])
    }

    /// Get progress percentage (0-100)
    pub fn progress(&self) -> u32 {
        if self.total_bytes == 0 {
            return 0;
        }
        ((self.bytes_read as f64 / self.total_bytes as f64) * 100.0) as u32
    }

    /// Get bytes read
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    /// Check if EOF reached
    pub fn is_eof(&self) -> bool {
        self.bytes_read >= self.total_bytes
    }
}

/// Process a file with streaming processors
pub fn process_file_streaming(
    reader: &mut dyn Read,
    processors: &mut [&mut dyn StreamingProcessor],
) -> io::Result<Vec<StreamingResult>> {
    let mut buffer = vec![0u8; 65536]; // 64KB buffer
    let start_time = std::time::Instant::now();

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }

        for processor in processors.iter_mut() {
            processor.process_chunk(&buffer[..bytes_read])?;
        }
    }

    let elapsed = start_time.elapsed().as_millis() as u64;

    let mut results = Vec::new();
    for processor in processors.iter_mut() {
        let mut result = processor.finalize()?;
        result.processing_time_ms = elapsed;
        results.push(result);
    }

    Ok(results)
}

#[cfg(test)]
#[path = "../tests/unit/streaming.rs"]
mod tests;
