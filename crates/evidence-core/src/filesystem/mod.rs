pub mod logical_fs;

use std::io::{self, Read, Seek};

const ROOT_NAME: &str = "\\";

#[derive(Debug, Clone)]
pub struct FsNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub hidden: bool,
    pub system: bool,
    /// True when the file is encrypted via NTFS Encrypting File System (EFS).
    /// Encrypted files cannot be read without the decryption key.
    pub encrypted: bool,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub modified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub accessed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Combined read + seek trait used by callers that need O(1) offset jumps.
pub trait ReadSeek: Read + Seek {}
impl<T> ReadSeek for T where T: Read + Seek {}

pub trait FileSystemReader {
    fn root(&self) -> io::Result<FsNode>;
    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>>;
    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>>;

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

/// Build the canonical root node returned by filesystem readers.
pub fn root_node() -> FsNode {
    fs_node_without_timestamps(ROOT_NAME, true, 0)
}

/// Build a filesystem node with an empty path ready for parent-path assignment.
pub fn fs_node(
    name: impl Into<String>,
    is_dir: bool,
    size: u64,
    created_at: Option<chrono::DateTime<chrono::Utc>>,
    modified_at: Option<chrono::DateTime<chrono::Utc>>,
    accessed_at: Option<chrono::DateTime<chrono::Utc>>,
) -> FsNode {
    fs_node_with_attributes(
        name,
        is_dir,
        size,
        false,
        false,
        false,
        created_at,
        modified_at,
        accessed_at,
    )
}

/// Build a filesystem node with explicit DOS/Windows-style hidden/system/encrypted flags.
#[allow(clippy::too_many_arguments)]
pub fn fs_node_with_attributes(
    name: impl Into<String>,
    is_dir: bool,
    size: u64,
    hidden: bool,
    system: bool,
    encrypted: bool,
    created_at: Option<chrono::DateTime<chrono::Utc>>,
    modified_at: Option<chrono::DateTime<chrono::Utc>>,
    accessed_at: Option<chrono::DateTime<chrono::Utc>>,
) -> FsNode {
    FsNode {
        name: name.into(),
        path: String::new(),
        is_dir,
        size,
        hidden,
        system,
        encrypted,
        created_at,
        modified_at,
        accessed_at,
    }
}

/// Build a filesystem node when the reader does not expose timestamps.
pub fn fs_node_without_timestamps(name: impl Into<String>, is_dir: bool, size: u64) -> FsNode {
    fs_node(name, is_dir, size, None, None, None)
}

/// Join a child name to a parent path while normalizing path separators.
///
/// The output uses `separator` and trims leading/trailing separators from the
/// parent. This keeps filesystem readers consistent without changing each
/// reader's externally visible separator convention.
pub fn join_child_path_with_separator(parent_path: &str, name: &str, separator: char) -> String {
    let normalized_parent = parent_path.replace(['\\', '/'], &separator.to_string());
    let parent = normalized_parent.trim_matches(separator);
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}{separator}{name}")
    }
}

/// Join a child name to a parent path using slash-separated paths.
pub fn join_child_path(parent_path: &str, name: &str) -> String {
    join_child_path_with_separator(parent_path, name, '/')
}

/// Assign a child path to an existing node while preserving its metadata.
pub fn node_with_parent_path_with_separator(
    mut node: FsNode,
    parent_path: &str,
    separator: char,
) -> FsNode {
    node.path = join_child_path_with_separator(parent_path, &node.name, separator);
    node
}

/// Assign a slash-separated child path to an existing node.
pub fn node_with_parent_path(node: FsNode, parent_path: &str) -> FsNode {
    node_with_parent_path_with_separator(node, parent_path, '/')
}

/// Assign parent paths to a collection of child nodes using `separator`.
pub fn child_nodes_with_parent_path_with_separator(
    nodes: impl IntoIterator<Item = FsNode>,
    parent_path: &str,
    separator: char,
) -> Vec<FsNode> {
    nodes
        .into_iter()
        .map(|node| node_with_parent_path_with_separator(node, parent_path, separator))
        .collect()
}

/// Assign slash-separated parent paths to a collection of child nodes.
pub fn child_nodes_with_parent_path(
    nodes: impl IntoIterator<Item = FsNode>,
    parent_path: &str,
) -> Vec<FsNode> {
    child_nodes_with_parent_path_with_separator(nodes, parent_path, '/')
}

/// Split a filesystem path into non-empty components using slash or backslash.
pub fn path_components(path: &str) -> Vec<&str> {
    path.trim_matches(['\\', '/'])
        .split(['\\', '/'])
        .filter(|component| !component.is_empty())
        .collect()
}

/// Return true for directory entries that should not be emitted as children.
pub fn is_special_directory_name(name: &str) -> bool {
    matches!(name, "." | "..")
}

/// Truncate file data to the filesystem's declared logical size.
///
/// Filesystem readers often read whole clusters and then trim the final buffer
/// to the file's valid data length. If the declared size is larger than this
/// platform can index, the buffer is left unchanged because it is necessarily
/// already smaller than the declared logical size.
pub fn truncate_data_to_declared_size(mut data: Vec<u8>, declared_size: u64) -> Vec<u8> {
    let Ok(limit) = usize::try_from(declared_size) else {
        return data;
    };
    data.truncate(data.len().min(limit));
    data
}

/// Return a standard not-found error for a filesystem path.
pub fn path_not_found(path: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("path not found: {path}"))
}

/// Return a standard not-found error for a filesystem file path.
pub fn file_not_found(path: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("file not found: {path}"))
}

/// Return a standard invalid-input error when a directory is used as a file.
pub fn path_is_directory(path: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{path} is a directory"),
    )
}

/// Return a standard invalid-input error when a file is used as a directory.
pub fn path_is_not_directory(path: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{path} is not a directory"),
    )
}

/// Return a standard invalid-data error for malformed filesystem structures.
pub fn invalid_fs_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Return a standard unsupported error for filesystem features outside this reader.
pub fn unsupported_fs(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, message.into())
}

/// Return a standard unexpected-EOF error for truncated filesystem structures.
pub fn unexpected_fs_eof(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, message.into())
}

/// Return a standard out-of-memory error for bounded filesystem reads.
pub fn fs_out_of_memory(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::OutOfMemory, message.into())
}

#[cfg(test)]
#[path = "../../tests/unit/filesystem.rs"]
mod tests;
