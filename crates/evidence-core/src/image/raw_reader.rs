use crate::reader::{EvidenceReader, ReaderInfo};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// Reader for raw (dd-style) disk images.
///
/// Wraps a `File` and exposes `Read + Seek`. Seek semantics are the standard
/// library's: seeking past EOF is legal and reads there return 0 bytes, while
/// seeking before the start returns an error rather than silently clamping.
#[derive(Debug)]
pub struct RawImageReader {
    file: File,
    info: ReaderInfo,
}

impl RawImageReader {
    /// Opens a raw image file at `path`.
    ///
    /// Returns an `io::Error` for non-existent files, permission problems (same
    /// semantics as `std::fs::File::open`), and directories.
    ///
    /// Directories are always rejected, but by two different paths depending on
    /// the platform. On Windows `File::open` already fails on a directory with
    /// `os error 5`, so the explicit check below is unreachable there. On Unix
    /// `File::open` succeeds on a directory and only the later read/seek fails,
    /// which would surface as a confusing error deep in the evidence path — the
    /// explicit check turns that into an immediate, named failure.
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let metadata = file.metadata()?;
        if metadata.is_dir() {
            return Err(io::Error::other(format!(
                "cannot open directory as raw image: {}",
                path.display()
            )));
        }
        Ok(Self {
            info: ReaderInfo {
                path: path.to_path_buf(),
                size: metadata.len(),
                kind: "raw".to_string(),
            },
            file,
        })
    }

    /// Returns the absolute path the reader was opened with.
    pub fn path(&self) -> &Path {
        &self.info.path
    }

    /// Returns the total size in bytes reported by the file system at open time.
    pub fn len(&self) -> u64 {
        self.info.size
    }

    /// Returns true when the image is empty (zero bytes).
    pub fn is_empty(&self) -> bool {
        self.info.size == 0
    }

    /// Creates an independent handle to the same image.
    ///
    /// The clone shares the same kernel file description, so it has its own
    /// seek position. This is fallible rather than a `Clone` impl so a failed
    /// `try_clone` surfaces as an error instead of a panic on an evidence path.
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            file: self.file.try_clone()?,
            info: self.info.clone(),
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

#[cfg(test)]
#[path = "../../tests/unit/image/raw_reader.rs"]
mod tests;
