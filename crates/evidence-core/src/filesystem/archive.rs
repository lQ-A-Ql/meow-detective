use crate::filesystem::{FileSystemDiagnostic, FileSystemReader, FsNode, ReadSeek};
use flate2::read::GzDecoder;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tar::Archive;

use super::archive_helpers::{
    gzip_size, gzip_stream_looks_like_tar, strip_gzip_suffix, InflatedLimit,
};
use super::archive_index::{
    build_children, enforce_entry_limit, index_tar_entry, normalize_lookup_path, ArchiveEntryMeta,
    MAX_ARCHIVE_INFLATED_SIZE, MAX_ENTRY_SIZE,
};
use super::archive_readers::{
    skip_exact, BoundedFileReader, GzipSingleEntryReader, GzipTarEntryReader,
};

const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    Tar,
    GzipTar,
    GzipSingle,
}

/// A read-only logical view over tar and gzip evidence without unpacking it.
pub struct ArchiveFsReader {
    source_path: PathBuf,
    data_source_name: String,
    kind: ArchiveKind,
    entries: BTreeMap<String, ArchiveEntryMeta>,
    children: BTreeMap<String, Vec<String>>,
    diagnostics: Mutex<Vec<FileSystemDiagnostic>>,
}

impl ArchiveFsReader {
    pub fn open(path: &Path, data_source_name: &str) -> io::Result<Self> {
        let metadata = std::fs::metadata(path)?;
        if !metadata.is_file() {
            return Err(invalid_data("archive source is not a regular file"));
        }
        let mut magic_file = File::open(path)?;
        let mut magic = [0u8; 2];
        let is_gzip = magic_file.read_exact(&mut magic).is_ok() && magic == GZIP_MAGIC;
        if is_gzip {
            Self::open_gzip(path, data_source_name)
        } else {
            Self::open_tar(path, data_source_name)
        }
    }

    fn open_tar(path: &Path, data_source_name: &str) -> io::Result<Self> {
        let file = File::open(path)?;
        let mut archive = Archive::new(file);
        let mut entries = BTreeMap::new();
        let mut skipped = 0usize;
        for item in archive.entries()? {
            let entry =
                item.map_err(|error| invalid_data(format!("invalid tar entry: {error}")))?;
            if index_tar_entry(&entry, &mut entries)? {
                skipped = skipped.saturating_add(1);
            }
            enforce_entry_limit(entries.len(), skipped)?;
        }
        Ok(Self::from_index(
            path,
            data_source_name,
            ArchiveKind::Tar,
            entries,
            skipped,
        ))
    }

    fn open_gzip(path: &Path, data_source_name: &str) -> io::Result<Self> {
        if gzip_stream_looks_like_tar(path)? {
            let file = File::open(path)?;
            let mut archive = Archive::new(InflatedLimit::new(
                GzDecoder::new(file),
                MAX_ARCHIVE_INFLATED_SIZE,
            ));
            let mut entries = BTreeMap::new();
            let mut skipped = 0usize;
            for item in archive.entries()? {
                let entry =
                    item.map_err(|error| invalid_data(format!("invalid gzip tar entry: {error}")))?;
                if index_tar_entry(&entry, &mut entries)? {
                    skipped = skipped.saturating_add(1);
                }
                enforce_entry_limit(entries.len(), skipped)?;
            }
            Ok(Self::from_index(
                path,
                data_source_name,
                ArchiveKind::GzipTar,
                entries,
                skipped,
            ))
        } else {
            let file_name = path
                .file_name()
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_else(|| "payload".to_string());
            let name = strip_gzip_suffix(&file_name);
            let size = gzip_size(path)?;
            if size > MAX_ENTRY_SIZE {
                return Err(invalid_data(
                    "gzip payload exceeds the per-entry size limit",
                ));
            }
            let mut entries = BTreeMap::new();
            entries.insert(
                name.clone(),
                ArchiveEntryMeta {
                    path: name.clone(),
                    name,
                    is_dir: false,
                    readable: true,
                    size,
                    data_offset: 0,
                    modified_at: None,
                    unix_mode: None,
                },
            );
            Ok(Self::from_index(
                path,
                data_source_name,
                ArchiveKind::GzipSingle,
                entries,
                0,
            ))
        }
    }

    fn from_index(
        source_path: &Path,
        data_source_name: &str,
        kind: ArchiveKind,
        mut entries: BTreeMap<String, ArchiveEntryMeta>,
        skipped: usize,
    ) -> Self {
        entries
            .entry(String::new())
            .or_insert_with(|| ArchiveEntryMeta {
                path: String::new(),
                name: source_path
                    .file_name()
                    .map(|value| value.to_string_lossy().into_owned())
                    .unwrap_or_else(|| data_source_name.to_string()),
                is_dir: true,
                readable: false,
                size: 0,
                data_offset: 0,
                modified_at: None,
                unix_mode: None,
            });
        let children = build_children(&entries);
        let diagnostics = if skipped == 0 {
            Vec::new()
        } else {
            vec![FileSystemDiagnostic::new(
                crate::filesystem::FileSystemDiagnosticKind::EntryUnavailable,
                format!("Skipped {skipped} unsupported archive entries"),
            )]
        };
        Self {
            source_path: source_path.to_path_buf(),
            data_source_name: data_source_name.to_string(),
            kind,
            entries,
            children,
            diagnostics: Mutex::new(diagnostics),
        }
    }

    fn lookup(&self, path: &str) -> io::Result<&ArchiveEntryMeta> {
        let normalized = normalize_lookup_path(path)?;
        self.entries
            .get(&normalized)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "archive path not found"))
    }

    fn node_for(&self, entry: &ArchiveEntryMeta) -> FsNode {
        FsNode {
            name: entry.name.clone(),
            path: entry.path.clone(),
            is_dir: entry.is_dir,
            size: entry.size,
            hidden: entry.name.starts_with('.'),
            system: false,
            read_only: true,
            encrypted: false,
            archive: false,
            unix_mode: entry.unix_mode,
            created_at: None,
            modified_at: entry.modified_at,
            accessed_at: None,
            changed_at: None,
        }
    }

    fn open_entry(&self, entry: &ArchiveEntryMeta) -> io::Result<Box<dyn Read>> {
        if entry.is_dir {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "archive path is a directory",
            ));
        }
        if !entry.readable {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "symbolic and hard links are not followed",
            ));
        }
        match self.kind {
            ArchiveKind::Tar => {
                let mut file = File::open(&self.source_path)?;
                file.seek(SeekFrom::Start(entry.data_offset))?;
                Ok(Box::new(file.take(entry.size)))
            }
            ArchiveKind::GzipTar => Ok(Box::new(GzipTarEntryReader::new(
                &self.source_path,
                entry.data_offset,
                entry.size,
            )?)),
            ArchiveKind::GzipSingle => Ok(Box::new(GzipSingleEntryReader::new(
                &self.source_path,
                entry.size,
            )?)),
        }
    }
}

impl FileSystemReader for ArchiveFsReader {
    fn root(&self) -> io::Result<FsNode> {
        self.entries
            .get("")
            .map(|entry| self.node_for(entry))
            .ok_or_else(|| invalid_data("archive root is missing"))
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        let normalized = normalize_lookup_path(path)?;
        let parent = self.lookup(&normalized)?;
        if !parent.is_dir {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "archive path is not a directory",
            ));
        }
        let mut result = self
            .children
            .get(&normalized)
            .into_iter()
            .flatten()
            .filter_map(|child| self.entries.get(child))
            .map(|entry| self.node_for(entry))
            .collect::<Vec<_>>();
        result.sort_by(|left, right| {
            right
                .is_dir
                .cmp(&left.is_dir)
                .then(left.name.to_lowercase().cmp(&right.name.to_lowercase()))
                .then(left.name.cmp(&right.name))
        });
        Ok(result)
    }

    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>> {
        let entry = self.lookup(path)?.clone();
        self.open_entry(&entry)
    }

    fn take_diagnostics(&self) -> Vec<FileSystemDiagnostic> {
        self.diagnostics
            .lock()
            .map(|mut diagnostics| std::mem::take(&mut *diagnostics))
            .unwrap_or_default()
    }

    fn read_file_range(&self, path: &str, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        let entry = self.lookup(path)?.clone();
        if entry.is_dir {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "archive path is a directory",
            ));
        }
        if offset >= entry.size || length == 0 {
            return Ok(Vec::new());
        }
        let requested = (entry.size - offset).min(length as u64);
        let capacity = usize::try_from(requested)
            .map_err(|_| invalid_data("requested archive range exceeds host memory limits"))?;
        let mut reader = self.open_entry(&entry)?;
        skip_exact(&mut reader, offset)?;
        let mut bytes = Vec::with_capacity(capacity);
        reader.take(requested).read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    fn open_file_seekable(&self, path: &str) -> io::Result<Box<dyn ReadSeek>> {
        let entry = self.lookup(path)?.clone();
        if entry.is_dir || !entry.readable {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "archive entry is not a readable regular file",
            ));
        }
        match self.kind {
            ArchiveKind::Tar => {
                let mut file = File::open(&self.source_path)?;
                file.seek(SeekFrom::Start(entry.data_offset))?;
                Ok(Box::new(BoundedFileReader::new(file, entry.size)))
            }
            ArchiveKind::GzipTar => Ok(Box::new(GzipTarEntryReader::new(
                &self.source_path,
                entry.data_offset,
                entry.size,
            )?)),
            ArchiveKind::GzipSingle => Ok(Box::new(GzipSingleEntryReader::new(
                &self.source_path,
                entry.size,
            )?)),
        }
    }

    fn data_source_name(&self) -> &str {
        &self.data_source_name
    }
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
