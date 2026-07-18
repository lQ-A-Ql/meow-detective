use std::collections::HashMap;
use std::io;

use evidence_core::filesystem::{
    invalid_fs_data, path_components, FileSystemDirectoryLocator, FileSystemFileLocator,
};

use crate::XfsReader;

const MAX_DIRECTORY_PATH_CACHE_ENTRIES: usize = 100_000;
const MAX_FILE_PATH_CACHE_ENTRIES: usize = 150_000;

impl XfsReader {
    pub(crate) fn exported_directory_locators(&self) -> Vec<FileSystemDirectoryLocator> {
        export_locators(&self.directory_path_cache.borrow(), true)
            .into_iter()
            .map(|(path, locator)| FileSystemDirectoryLocator { path, locator })
            .collect()
    }

    pub(crate) fn exported_file_locators(&self) -> Vec<FileSystemFileLocator> {
        export_locators(&self.file_path_cache.borrow(), false)
            .into_iter()
            .map(|(path, locator)| FileSystemFileLocator { path, locator })
            .collect()
    }

    pub(crate) fn seed_persisted_directory_locators(
        &self,
        locators: &[FileSystemDirectoryLocator],
    ) -> io::Result<()> {
        let parsed = parse_locators(
            locators
                .iter()
                .map(|locator| (&locator.path, &locator.locator)),
            MAX_DIRECTORY_PATH_CACHE_ENTRIES.saturating_sub(1),
            "directory",
        )?;
        seed_cache(
            &mut self.directory_path_cache.borrow_mut(),
            parsed,
            MAX_DIRECTORY_PATH_CACHE_ENTRIES,
        );
        Ok(())
    }

    pub(crate) fn seed_persisted_file_locators(
        &self,
        locators: &[FileSystemFileLocator],
    ) -> io::Result<()> {
        let parsed = parse_locators(
            locators
                .iter()
                .map(|locator| (&locator.path, &locator.locator)),
            MAX_FILE_PATH_CACHE_ENTRIES,
            "file",
        )?;
        let inserted = seed_cache(
            &mut self.file_path_cache.borrow_mut(),
            parsed,
            MAX_FILE_PATH_CACHE_ENTRIES,
        );
        self.unverified_file_path_cache
            .borrow_mut()
            .extend(inserted);
        Ok(())
    }

    pub(crate) fn cache_directory_path(&self, path: String, inode: u64) {
        cache_path(
            &mut self.directory_path_cache.borrow_mut(),
            path,
            inode,
            MAX_DIRECTORY_PATH_CACHE_ENTRIES,
        );
    }

    pub(crate) fn cache_file_path(&self, path: String, inode: u64) {
        self.unverified_file_path_cache.borrow_mut().remove(&path);
        cache_path(
            &mut self.file_path_cache.borrow_mut(),
            path,
            inode,
            MAX_FILE_PATH_CACHE_ENTRIES,
        );
    }

    pub(crate) fn resolve_cached_file_locator(&self, path: &str) -> Option<(u64, Vec<u8>, bool)> {
        let inode = self.file_path_cache.borrow().get(path).copied()?;
        let bytes = self.read_inode(inode).and_then(|bytes| {
            Self::validate_inode_magic(&bytes)?;
            if Self::inode_is_dir(&bytes) {
                return Err(invalid_fs_data(format!(
                    "cached XFS file locator for '{path}' references a directory"
                )));
            }
            Ok(bytes)
        });
        if let Ok(bytes) = bytes {
            let requires_binding_validation =
                self.unverified_file_path_cache.borrow().contains(path);
            return Some((inode, bytes, requires_binding_validation));
        }
        self.discard_cached_file_locator(path);
        None
    }

    pub(crate) fn mark_cached_file_locator_verified(&self, path: &str) {
        self.unverified_file_path_cache.borrow_mut().remove(path);
    }

    pub(crate) fn discard_cached_file_locator(&self, path: &str) {
        self.file_path_cache.borrow_mut().remove(path);
        self.unverified_file_path_cache.borrow_mut().remove(path);
    }
}

fn export_locators(cache: &HashMap<String, u64>, omit_root: bool) -> Vec<(String, String)> {
    let mut locators = cache
        .iter()
        .filter(|(path, _)| !omit_root || !path.is_empty())
        .map(|(path, inode)| (path.clone(), inode.to_string()))
        .collect::<Vec<_>>();
    locators.sort_by(|left, right| left.0.cmp(&right.0));
    locators
}

fn parse_locators<'a>(
    locators: impl IntoIterator<Item = (&'a String, &'a String)>,
    limit: usize,
    kind: &str,
) -> io::Result<Vec<(String, u64)>> {
    let mut parsed = Vec::new();
    for (path, locator) in locators.into_iter().take(limit) {
        let components = path_components(path);
        if components.is_empty()
            || components
                .iter()
                .any(|component| matches!(*component, "." | "..") || component.contains('\0'))
        {
            return Err(invalid_fs_data(format!(
                "persisted XFS {kind} locator has an invalid path"
            )));
        }
        let inode = locator.parse::<u64>().map_err(|_| {
            invalid_fs_data(format!("persisted XFS {kind} locator has an invalid inode"))
        })?;
        if inode == 0 {
            return Err(invalid_fs_data(format!(
                "persisted XFS {kind} locator references inode 0"
            )));
        }
        parsed.push((components.join("/"), inode));
    }
    Ok(parsed)
}

fn seed_cache(
    cache: &mut HashMap<String, u64>,
    locators: Vec<(String, u64)>,
    limit: usize,
) -> Vec<String> {
    let mut inserted = Vec::new();
    for (path, inode) in locators {
        if cache.len() >= limit {
            break;
        }
        if let std::collections::hash_map::Entry::Vacant(entry) = cache.entry(path) {
            inserted.push(entry.key().clone());
            entry.insert(inode);
        }
    }
    inserted
}

fn cache_path(cache: &mut HashMap<String, u64>, path: String, inode: u64, limit: usize) {
    if inode != 0 && cache.len() < limit {
        cache.insert(path, inode);
    }
}
