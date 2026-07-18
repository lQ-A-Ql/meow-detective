mod diagnostic;
mod errors;
pub mod logical_fs;
mod node;
mod path;

use std::io::{self, Read, Seek};

pub use diagnostic::{FileSystemDiagnostic, FileSystemDiagnosticKind};
pub use errors::{
    file_not_found, fs_out_of_memory, invalid_fs_data, path_is_directory, path_is_not_directory,
    path_not_found, unexpected_fs_eof, unsupported_fs,
};
pub use node::{
    fs_node, fs_node_with_attributes, fs_node_without_timestamps, root_node,
    truncate_data_to_declared_size, FileSystemDirectoryLocator, FileSystemFileLocator, FsNode,
    FsTimestamp,
};
pub use path::{
    child_nodes_with_parent_path, child_nodes_with_parent_path_with_separator,
    is_special_directory_name, join_child_path, join_child_path_with_separator,
    node_with_parent_path, node_with_parent_path_with_separator, path_components,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileSystemReadMetrics {
    pub filesystem_open_operations: u64,
    pub metadata_cache_hits: u64,
    pub metadata_cache_misses: u64,
    pub evidence_read_operations: u64,
    pub evidence_bytes_read: u64,
}

impl FileSystemReadMetrics {
    pub fn merge(&mut self, other: Self) {
        self.filesystem_open_operations = self
            .filesystem_open_operations
            .saturating_add(other.filesystem_open_operations);
        self.metadata_cache_hits = self
            .metadata_cache_hits
            .saturating_add(other.metadata_cache_hits);
        self.metadata_cache_misses = self
            .metadata_cache_misses
            .saturating_add(other.metadata_cache_misses);
        self.evidence_read_operations = self
            .evidence_read_operations
            .saturating_add(other.evidence_read_operations);
        self.evidence_bytes_read = self
            .evidence_bytes_read
            .saturating_add(other.evidence_bytes_read);
    }
}

/// Combined read + seek trait used by callers that need O(1) offset jumps.
pub trait ReadSeek: Read + Seek {}
impl<T> ReadSeek for T where T: Read + Seek {}

pub trait FileSystemReader {
    fn root(&self) -> io::Result<FsNode>;
    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>>;
    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>>;

    /// Drain non-fatal diagnostics produced while reading the filesystem.
    ///
    /// Readers may use this for entry-local corruption that should not hide
    /// otherwise valid siblings. The import layer persists these messages as
    /// enumeration warnings instead of silently fabricating metadata.
    fn take_diagnostics(&self) -> Vec<FileSystemDiagnostic> {
        Vec::new()
    }

    /// Export stable directory-path locators discovered while walking this
    /// filesystem. Callers may persist these as a performance hint.
    fn directory_locators(&self) -> Vec<FileSystemDirectoryLocator> {
        Vec::new()
    }

    /// Seed stable directory-path locators from a trusted, source-local cache.
    ///
    /// Implementations must continue to validate inode/block metadata when a
    /// locator is used. These locators are acceleration hints, not evidence.
    fn seed_directory_locators(&self, _locators: &[FileSystemDirectoryLocator]) -> io::Result<()> {
        Ok(())
    }

    /// Export stable file-path locators discovered while walking this
    /// filesystem. Callers may persist these as a bounded performance hint.
    fn file_locators(&self) -> Vec<FileSystemFileLocator> {
        Vec::new()
    }

    /// Seed stable file-path locators from a trusted, source-local cache.
    ///
    /// Implementations must validate the referenced filesystem identity when
    /// a locator is used and fall back to normal path resolution on mismatch.
    fn seed_file_locators(&self, _locators: &[FileSystemFileLocator]) -> io::Result<()> {
        Ok(())
    }

    /// Read a bounded byte range without requiring callers to materialize the
    /// entire file. Filesystems that cannot provide an efficient range path
    /// should leave this default and let callers fall back explicitly.
    fn read_file_range(&self, _path: &str, _offset: u64, _length: usize) -> io::Result<Vec<u8>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "range file access is not implemented for this filesystem",
        ))
    }

    /// Open a file with guaranteed seek support. Readers whose underlying
    /// implementation is not seekable (for example streaming decompressors)
    /// should leave the default implementation, which returns
    /// `Unsupported` so callers can fall back to sequential reads.
    fn open_file_seekable(&self, _path: &str) -> io::Result<Box<dyn ReadSeek>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "seekable file access is not implemented for this filesystem",
        ))
    }

    fn read_metrics(&self) -> FileSystemReadMetrics {
        FileSystemReadMetrics::default()
    }

    fn data_source_name(&self) -> &str;
}

#[cfg(test)]
#[path = "../../tests/unit/filesystem.rs"]
mod tests;
