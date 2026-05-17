//! NTFS filesystem reader stub.
//! Full NTFS parsing ($MFT, $Bitmap, INDX, etc.) is future work.

use evidence_core::filesystem::{FileSystemReader, FsNode};
use std::io::{self, Read};
use std::path::Path;

pub struct NtfsReader;

impl NtfsReader {
    pub fn open(_image: &Path, _offset: u64) -> io::Result<Self> {
        Ok(Self)
    }
}

impl FileSystemReader for NtfsReader {
    fn root(&self) -> io::Result<FsNode> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "NTFS reader not yet implemented"))
    }
    fn list_children(&self, _path: &str) -> io::Result<Vec<FsNode>> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "NTFS reader not yet implemented"))
    }
    fn open_file(&self, _path: &str) -> io::Result<Box<dyn Read>> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "NTFS reader not yet implemented"))
    }
    fn data_source_name(&self) -> &str { "NTFS" }
}
