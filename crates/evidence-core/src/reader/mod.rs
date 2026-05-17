use std::io::{Read, Seek};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ReaderInfo {
    pub path: PathBuf,
    pub size: u64,
    pub kind: String,
}

pub trait EvidenceReader: Read + Seek {
    fn info(&self) -> &ReaderInfo;
}
