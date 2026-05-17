pub mod logical_fs;

use std::io::{self, Read};

#[derive(Debug, Clone)]
pub struct FsNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub modified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub accessed_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub trait FileSystemReader {
    fn root(&self) -> io::Result<FsNode>;
    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>>;
    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>>;
    fn data_source_name(&self) -> &str;
}
