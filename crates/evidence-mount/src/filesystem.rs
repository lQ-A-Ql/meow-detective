use std::io;
use std::time::SystemTime;

use crate::{MountError, MountPath};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountNode {
    pub path: MountPath,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub read_only: bool,
    pub hidden: bool,
    pub system: bool,
    pub encrypted: bool,
    pub created_at: Option<SystemTime>,
    pub modified_at: Option<SystemTime>,
    pub accessed_at: Option<SystemTime>,
    pub source_file_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryPage {
    pub entries: Vec<MountNode>,
    pub next_cursor: Option<String>,
}

pub trait MountFileHandle: Send {
    fn size(&self) -> u64;
    fn read_at(&mut self, offset: u64, length: usize) -> io::Result<Vec<u8>>;
}

pub trait MountFileSystem: Send + Sync {
    fn lookup(&self, path: &MountPath) -> Result<MountNode, MountError>;

    fn read_directory(
        &self,
        path: &MountPath,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<DirectoryPage, MountError>;

    fn open_read(&self, path: &MountPath) -> Result<Box<dyn MountFileHandle>, MountError>;
}
