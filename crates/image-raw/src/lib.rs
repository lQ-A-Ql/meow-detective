use evidence_core::{EvidenceReader, ReaderInfo};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// Reader for raw (dd-style) disk images.
///
/// Wraps a `File` and exposes `Read + Seek` with a guarded `SeekFrom::End`
/// that clamps to the file length reported at open time.
#[derive(Debug)]
pub struct RawImageReader {
    file: File,
    info: ReaderInfo,
}

impl RawImageReader {
    /// Opens a raw image file at `path`.
    ///
    /// Returns an `io::Error` for non-existent files, directories, or permission
    /// problems (same semantics as `std::fs::File::open`).
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let metadata = file.metadata()?;
        // Reject directories explicitly (File::open succeeds on directories on some
        // platforms but read/seek will fail with confusing errors later).
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
}

impl Read for RawImageReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read(buf)
    }
}

impl Seek for RawImageReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        // Guard SeekFrom::End against going negative so the wrapped file seek
        // receives a valid absolute position.
        match pos {
            SeekFrom::End(offset) => {
                let abs = if offset >= 0 {
                    self.info.size.saturating_add(offset as u64)
                } else {
                    self.info.size.saturating_sub(offset.unsigned_abs())
                };
                self.file.seek(SeekFrom::Start(abs))
            }
            other => self.file.seek(other),
        }
    }
}

impl EvidenceReader for RawImageReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

/// Supports shallow clones (via `File::try_clone`). The clone shares the same
/// kernel file description (independent seek position), which is the usual
/// expectation for evidence readers.
impl Clone for RawImageReader {
    fn clone(&self) -> Self {
        Self {
            file: self
                .file
                .try_clone()
                .expect("RawImageReader::clone: try_clone failed"),
            info: self.info.clone(),
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/image_raw.rs"]
mod tests;
