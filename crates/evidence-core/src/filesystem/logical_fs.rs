use crate::filesystem::{FileSystemReader, FsNode};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub struct LogicalFsReader {
    root: PathBuf,
    data_source_name: String,
}

impl LogicalFsReader {
    pub fn open(path: &Path, data_source_name: &str) -> io::Result<Self> {
        let canonical = path.canonicalize()?;
        Ok(Self {
            root: canonical,
            data_source_name: data_source_name.to_string(),
        })
    }

    fn to_relative(&self, full: &Path) -> String {
        full.strip_prefix(&self.root)
            .map(path_to_relative_string)
            .unwrap_or_else(|_| full.display().to_string())
    }

    fn to_full(&self, relative: &str) -> PathBuf {
        if relative.is_empty() {
            self.root.clone()
        } else {
            self.root.join(relative)
        }
    }

    fn node_from_entry(&self, entry: &fs::DirEntry) -> io::Result<FsNode> {
        let metadata = entry.metadata()?;
        let full = entry.path();
        Ok(FsNode {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: self.to_relative(&full),
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            created_at: system_time_to_utc(metadata.created().ok()),
            modified_at: system_time_to_utc(metadata.modified().ok()),
            accessed_at: system_time_to_utc(metadata.accessed().ok()),
        })
    }
}

impl FileSystemReader for LogicalFsReader {
    fn root(&self) -> io::Result<FsNode> {
        let metadata = self.root.metadata()?;
        Ok(FsNode {
            name: self
                .root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.data_source_name.clone()),
            path: String::new(),
            is_dir: true,
            size: 0,
            created_at: system_time_to_utc(metadata.created().ok()),
            modified_at: system_time_to_utc(metadata.modified().ok()),
            accessed_at: system_time_to_utc(metadata.accessed().ok()),
        })
    }

    fn list_children(&self, relative_path: &str) -> io::Result<Vec<FsNode>> {
        let full = self.to_full(relative_path);
        let dir = fs::read_dir(&full)?;
        let mut children = Vec::new();
        for entry in dir {
            let entry = entry?;
            children.push(self.node_from_entry(&entry)?);
        }
        children.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(children)
    }

    fn open_file(&self, relative_path: &str) -> io::Result<Box<dyn Read>> {
        let full = self.to_full(relative_path);
        Ok(Box::new(fs::File::open(full)?))
    }

    fn data_source_name(&self) -> &str {
        &self.data_source_name
    }
}

fn system_time_to_utc(st: Option<std::time::SystemTime>) -> Option<chrono::DateTime<chrono::Utc>> {
    st.and_then(|t| {
        t.duration_since(UNIX_EPOCH).ok().map(|d| {
            chrono::DateTime::from_timestamp(d.as_secs() as i64, d.subsec_nanos())
                .unwrap_or_default()
        })
    })
}

fn path_to_relative_string(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}
