use crate::reader::ExfatReader;
use evidence_core::filesystem::{
    file_not_found, path_components, path_is_directory, truncate_data_to_declared_size,
};
use std::io;

pub(crate) struct ResolvedEntry {
    pub(crate) cluster: u32,
    pub(crate) is_dir: bool,
    pub(crate) size: u64,
    pub(crate) data_length: u64,
    pub(crate) no_fat_chain: bool,
}

impl ExfatReader {
    pub(crate) fn resolve_path(&self, path: &str) -> io::Result<Option<ResolvedEntry>> {
        let components = path_components(path);
        if components.is_empty() {
            return Ok(Some(ResolvedEntry {
                cluster: self.boot.first_cluster_of_root,
                is_dir: true,
                size: 0,
                data_length: 0,
                no_fat_chain: false,
            }));
        }

        let mut resolved = ResolvedEntry {
            cluster: self.boot.first_cluster_of_root,
            is_dir: true,
            size: 0,
            data_length: 0,
            no_fat_chain: false,
        };
        for component in components {
            if !resolved.is_dir {
                return Ok(None);
            }

            let entries = self.read_directory_entries(
                resolved.cluster,
                resolved.no_fat_chain,
                resolved.data_length,
            )?;
            let found = entries
                .into_iter()
                .find(|entry| self.names_equal(&entry.name, component));
            let Some(entry) = found else {
                return Ok(None);
            };

            resolved = ResolvedEntry {
                cluster: entry.first_cluster,
                is_dir: entry.is_directory(),
                size: if entry.is_directory() {
                    0
                } else {
                    entry.valid_data_length
                },
                data_length: entry.data_length,
                no_fat_chain: entry.no_fat_chain,
            };
        }

        Ok(Some(resolved))
    }

    /// Read a file range by path without materializing the whole file.
    pub fn read_file_range(&self, path: &str, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        let entry = self
            .resolve_path(path)?
            .ok_or_else(|| file_not_found(path))?;
        if entry.is_dir {
            return Err(path_is_directory(path));
        }

        self.read_entry_range(
            entry.cluster,
            entry.size,
            entry.no_fat_chain,
            offset,
            length,
        )
    }

    pub(crate) fn open_file_cursor(&self, path: &str) -> io::Result<io::Cursor<Vec<u8>>> {
        let entry = self
            .resolve_path(path)?
            .ok_or_else(|| file_not_found(path))?;
        if entry.is_dir {
            return Err(path_is_directory(path));
        }

        let data = truncate_data_to_declared_size(
            self.read_entry_data(entry.cluster, entry.size, entry.no_fat_chain)?,
            entry.size,
        );
        Ok(io::Cursor::new(data))
    }
}
