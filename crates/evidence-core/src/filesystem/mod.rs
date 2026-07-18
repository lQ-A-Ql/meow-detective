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
    truncate_data_to_declared_size, FsNode, FsTimestamp,
};
pub use path::{
    child_nodes_with_parent_path, child_nodes_with_parent_path_with_separator,
    is_special_directory_name, join_child_path, join_child_path_with_separator,
    node_with_parent_path, node_with_parent_path_with_separator, path_components,
};

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

    fn data_source_name(&self) -> &str;
}

#[cfg(test)]
#[path = "../../tests/unit/filesystem.rs"]
mod tests;
