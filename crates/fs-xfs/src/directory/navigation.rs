use super::{
    XfsDirectoryEntry, XfsResolvedDirectoryEntry, DIR2_SF_HDR_4, DIR2_SF_HDR_8, XFS_DIR3_FT_DIR,
    XFS_DIR3_FT_MAX, XFS_DIR3_FT_UNKNOWN,
};
use crate::inode::XfsInodeMetadata;
use crate::{XfsReader, FORMAT_BTREE, FORMAT_EXTENTS, FORMAT_LOCAL};
use evidence_core::filesystem::{
    invalid_fs_data, path_components, FileSystemDiagnostic, FileSystemDiagnosticKind,
};
use std::io;
use std::sync::Arc;

const MAX_DIRECTORY_INODE_CACHE_ENTRIES: usize = 32_768;
const MAX_FILESYSTEM_DIAGNOSTICS: usize = 1_000;

pub(crate) struct XfsResolvedPath {
    pub(crate) inode_number: u64,
    pub(crate) is_dir: bool,
    pub(crate) inode: Option<Vec<u8>>,
}

impl XfsReader {
    pub(crate) fn read_directory_entries(
        &self,
        ino: u64,
    ) -> io::Result<Vec<XfsResolvedDirectoryEntry>> {
        let inode = self
            .directory_inode_cache
            .borrow()
            .get(&ino)
            .cloned()
            .map_or_else(|| self.read_inode(ino), Ok)?;
        Self::validate_inode_magic(&inode)?;
        if !Self::inode_is_dir(&inode) {
            return Err(invalid_fs_data(format!("inode {ino} is not a directory")));
        }

        let entries = self.raw_directory_entries(ino, &inode)?;
        Ok(self.annotate_directory_entries(entries.iter().cloned()))
    }

    fn raw_directory_entries(
        &self,
        ino: u64,
        inode: &[u8],
    ) -> io::Result<Arc<Vec<XfsDirectoryEntry>>> {
        if let Some(entries) = self.directory_entry_cache.borrow_mut().get(ino) {
            return Ok(entries);
        }

        let diagnostic_count = self.diagnostics.borrow().len();
        let format = Self::inode_format(inode)?;
        let (entries, cacheable) = match format {
            FORMAT_LOCAL => {
                let data_fork = Self::data_fork(inode)?;
                let raw = Self::parse_shortform_dir(data_fork, self.has_ftype)?;
                (
                    Self::shortform_entries_to_directory_entries(raw, self.has_ftype, data_fork),
                    true,
                )
            }
            FORMAT_EXTENTS => match self.read_extent_directory_entries(inode) {
                Ok(entries) => (entries, true),
                Err(block_error) => {
                    if !self
                        .extent_directory_data_is_all_zero(inode)
                        .unwrap_or(false)
                    {
                        return Err(block_error);
                    }
                    let data_fork = Self::data_fork(inode)?;
                    let full_literal =
                        inode.get(Self::inode_core_size(inode)..).ok_or_else(|| {
                            invalid_fs_data(format!(
                                "directory inode {ino} buffer is shorter than its core"
                            ))
                        })?;
                    let recovered = self
                        .recover_shortform_dir_entries_raw(
                            &[data_fork, full_literal],
                            self.has_ftype,
                        )
                        .ok_or_else(|| {
                            invalid_fs_data(
                                "block dir all-zero (sf->block conversion artifact), recovery failed",
                            )
                        })?;
                    (recovered, false)
                }
            },
            FORMAT_BTREE => {
                let entries = self.read_btree_directory_entries(inode)?;
                (entries, true)
            }
            other => {
                return Err(invalid_fs_data(format!(
                    "directory inode {ino} uses unsupported format {other}"
                )))
            }
        };
        let entries = Arc::new(entries);
        if cacheable && self.diagnostics.borrow().len() == diagnostic_count {
            self.directory_entry_cache
                .borrow_mut()
                .insert(ino, Arc::clone(&entries));
        }
        Ok(entries)
    }

    pub(super) fn recover_shortform_dir_entries_raw(
        &self,
        slices: &[&[u8]],
        prefer_ftype: bool,
    ) -> Option<Vec<XfsDirectoryEntry>> {
        for slice in slices {
            if let Some(entries) = self.try_shortform_dir_scan(slice, prefer_ftype) {
                return Some(entries);
            }
            if let Some(entries) = self.try_shortform_dir_scan(slice, !prefer_ftype) {
                return Some(entries);
            }
        }
        None
    }

    fn try_shortform_dir_scan(
        &self,
        slice: &[u8],
        has_ftype: bool,
    ) -> Option<Vec<XfsDirectoryEntry>> {
        for start in 0..slice.len().saturating_sub(DIR2_SF_HDR_4) {
            let count = usize::from(slice[start]);
            if count == 0 || count > 128 {
                continue;
            }
            let i8count = usize::from(slice[start + 1]);
            if i8count > count {
                continue;
            }
            let raw = match Self::parse_shortform_dir(&slice[start..], has_ftype) {
                Ok(raw) if raw.len() == count => raw,
                _ => continue,
            };
            if raw
                .iter()
                .all(|(name, inode)| is_plausible_shortform_name(name) && *inode > 0)
            {
                return Some(Self::shortform_entries_to_directory_entries(
                    raw,
                    has_ftype,
                    &slice[start..],
                ));
            }
        }
        None
    }

    fn shortform_entries_to_directory_entries(
        raw: Vec<(String, u64)>,
        has_ftype: bool,
        data: &[u8],
    ) -> Vec<XfsDirectoryEntry> {
        let file_types = Self::parse_shortform_dir_ftypes(data, has_ftype).unwrap_or_default();
        raw.into_iter()
            .enumerate()
            .map(|(index, (name, inode))| XfsDirectoryEntry {
                name,
                inode,
                ftype: file_types.get(index).copied().flatten(),
            })
            .collect()
    }

    fn parse_shortform_dir_ftypes(
        data_fork: &[u8],
        has_ftype: bool,
    ) -> io::Result<Vec<Option<u8>>> {
        if data_fork.len() < DIR2_SF_HDR_4 {
            return Err(invalid_fs_data("shortform dir too small for header"));
        }
        let count = usize::from(data_fork[0]);
        let i8count = usize::from(data_fork[1]);
        let header_size = if i8count == 0 {
            DIR2_SF_HDR_4
        } else {
            DIR2_SF_HDR_8
        };
        if data_fork.len() < header_size {
            return Err(invalid_fs_data("shortform dir header truncated"));
        }

        let mut position = header_size;
        let mut file_types = Vec::with_capacity(count);
        for _ in 0..count {
            if position + 3 > data_fork.len() {
                break;
            }
            let name_len = usize::from(data_fork[position]);
            let name_end = position + 3 + name_len;
            if name_len == 0 || name_end > data_fork.len() {
                break;
            }
            let inode_len = if i8count != 0 { 8 } else { 4 };
            let file_type = if has_ftype {
                if name_end + 1 + inode_len > data_fork.len() {
                    break;
                }
                Some(data_fork[name_end]).filter(|value| *value < XFS_DIR3_FT_MAX)
            } else {
                if name_end + inode_len > data_fork.len() {
                    break;
                }
                None
            };
            file_types.push(file_type);
            position = name_end + inode_len + usize::from(has_ftype);
        }
        Ok(file_types)
    }

    fn annotate_directory_entries(
        &self,
        raw: impl IntoIterator<Item = XfsDirectoryEntry>,
    ) -> Vec<XfsResolvedDirectoryEntry> {
        raw.into_iter()
            .filter_map(|entry| {
                let fallback_is_dir = dirent_is_dir(entry.ftype);
                match self.child_inode_metadata(entry.inode) {
                    Ok((metadata, _)) => {
                        if let Err(error) =
                            validate_dirent_type(entry.inode, entry.ftype, metadata.is_dir)
                        {
                            self.record_diagnostic(
                                FileSystemDiagnostic::new(
                                    FileSystemDiagnosticKind::TypeConflict,
                                    error.to_string(),
                                )
                                .with_inode(entry.inode),
                            );
                        }
                        Some(XfsResolvedDirectoryEntry {
                            name: entry.name,
                            inode: entry.inode,
                            is_dir: metadata.is_dir,
                            metadata: Some(metadata),
                        })
                    }
                    Err(error) => {
                        let Some(is_dir) = fallback_is_dir else {
                            self.record_diagnostic(
                                FileSystemDiagnostic::new(
                                    FileSystemDiagnosticKind::EntryUnavailable,
                                    format!(
                                        "XFS directory entry '{}' inode {} was omitted because its metadata is unreadable and its file type is unknown: {}",
                                        entry.name, entry.inode, error
                                    ),
                                )
                                .with_inode(entry.inode),
                            );
                            return None;
                        };
                        self.record_diagnostic(
                            FileSystemDiagnostic::new(
                                FileSystemDiagnosticKind::MetadataDegraded,
                                format!(
                                    "XFS directory entry '{}' inode {} retained with directory-entry type but without inode metadata: {}",
                                    entry.name, entry.inode, error
                                ),
                            )
                            .with_inode(entry.inode),
                        );
                        Some(XfsResolvedDirectoryEntry {
                            name: entry.name,
                            inode: entry.inode,
                            is_dir,
                            metadata: None,
                        })
                    }
                }
            })
            .collect()
    }

    pub(crate) fn record_diagnostic(&self, diagnostic: FileSystemDiagnostic) {
        let mut diagnostics = self.diagnostics.borrow_mut();
        if diagnostics.len() < MAX_FILESYSTEM_DIAGNOSTICS {
            diagnostics.push(diagnostic);
        }
    }

    fn child_inode_metadata(&self, ino: u64) -> io::Result<(XfsInodeMetadata, Vec<u8>)> {
        let inode = self.read_inode(ino).map_err(|error| {
            invalid_fs_data(format!("cannot read directory child inode {ino}: {error}"))
        })?;
        let metadata = self
            .decode_inode_metadata_with_diagnostics(ino, &inode)
            .map_err(|error| {
                invalid_fs_data(format!(
                    "cannot decode directory child inode {ino} metadata: {error}"
                ))
            })?;
        Self::validate_directory_inode_metadata(ino, &inode, &metadata)?;
        if metadata.is_dir {
            let mut cache = self.directory_inode_cache.borrow_mut();
            if cache.len() < MAX_DIRECTORY_INODE_CACHE_ENTRIES {
                cache.entry(ino).or_insert_with(|| inode.clone());
            }
        }
        Ok((metadata, inode))
    }

    fn lookup_directory_entry(&self, ino: u64, name: &str) -> io::Result<Option<XfsResolvedPath>> {
        let inode = self
            .directory_inode_cache
            .borrow()
            .get(&ino)
            .cloned()
            .map_or_else(|| self.read_inode(ino), Ok)?;
        Self::validate_inode_magic(&inode)?;
        if !Self::inode_is_dir(&inode) {
            return Err(invalid_fs_data(format!("inode {ino} is not a directory")));
        }

        let entries = self.raw_directory_entries(ino, &inode)?;
        let Some(entry) = entries.iter().find(|entry| entry.name == name) else {
            return Ok(None);
        };
        let (metadata, inode) = self.child_inode_metadata(entry.inode)?;
        validate_dirent_type(entry.inode, entry.ftype, metadata.is_dir)?;
        Ok(Some(XfsResolvedPath {
            inode_number: entry.inode,
            is_dir: metadata.is_dir,
            inode: Some(inode),
        }))
    }

    pub(crate) fn resolve_path(&self, path: &str) -> io::Result<Option<(u64, bool)>> {
        Ok(self
            .resolve_path_with_inode(path)?
            .map(|resolved| (resolved.inode_number, resolved.is_dir)))
    }

    pub(crate) fn resolve_path_with_inode(
        &self,
        path: &str,
    ) -> io::Result<Option<XfsResolvedPath>> {
        let components = path_components(path);
        if components.is_empty() {
            return Ok(Some(XfsResolvedPath {
                inode_number: self.root_ino,
                is_dir: true,
                inode: None,
            }));
        }
        let normalized_path = components.join("/");
        if let Some((inode_number, inode, requires_binding_validation)) =
            self.resolve_cached_file_locator(&normalized_path)
        {
            if !requires_binding_validation
                || self.persisted_file_locator_matches_path(&components, inode_number)?
            {
                self.mark_cached_file_locator_verified(&normalized_path);
                return Ok(Some(XfsResolvedPath {
                    inode_number,
                    is_dir: false,
                    inode: Some(inode),
                }));
            }
            self.discard_cached_file_locator(&normalized_path);
        }
        if let Some(inode_number) = self
            .directory_path_cache
            .borrow()
            .get(&normalized_path)
            .copied()
        {
            return Ok(Some(XfsResolvedPath {
                inode_number,
                is_dir: true,
                inode: None,
            }));
        }

        let (mut current_inode, mut current_path, start_index) =
            self.longest_cached_directory_prefix(&components);
        for (index, component) in components.iter().enumerate().skip(start_index) {
            let is_last = index == components.len() - 1;
            let Some(resolved) = self.lookup_directory_entry(current_inode, component)? else {
                return Ok(None);
            };
            current_path = if current_path.is_empty() {
                (*component).to_string()
            } else {
                format!("{current_path}/{component}")
            };
            if resolved.is_dir {
                self.cache_directory_path(current_path.clone(), resolved.inode_number);
            }
            if is_last {
                if !resolved.is_dir {
                    self.cache_file_path(current_path, resolved.inode_number);
                }
                return Ok(Some(resolved));
            }
            if !resolved.is_dir {
                return Ok(None);
            }
            current_inode = resolved.inode_number;
        }
        Ok(None)
    }

    fn persisted_file_locator_matches_path(
        &self,
        components: &[&str],
        expected_inode: u64,
    ) -> io::Result<bool> {
        let Some(final_name) = components.last().copied() else {
            return Ok(false);
        };
        let (mut current_inode, _, start_index) = self.longest_cached_directory_prefix(components);
        for component in components
            .iter()
            .copied()
            .take(components.len().saturating_sub(1))
            .skip(start_index)
        {
            let Some(resolved) = self.lookup_directory_entry(current_inode, component)? else {
                return Ok(false);
            };
            if !resolved.is_dir {
                return Ok(false);
            }
            current_inode = resolved.inode_number;
        }
        let Some(resolved) = self.lookup_directory_entry(current_inode, final_name)? else {
            return Ok(false);
        };
        Ok(!resolved.is_dir && resolved.inode_number == expected_inode)
    }

    fn longest_cached_directory_prefix(&self, components: &[&str]) -> (u64, String, usize) {
        let cache = self.directory_path_cache.borrow();
        let mut prefix = String::new();
        let mut longest = (self.root_ino, String::new(), 0usize);
        for (index, component) in components
            .iter()
            .enumerate()
            .take(components.len().saturating_sub(1))
        {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(component);
            if let Some(inode) = cache.get(&prefix).copied() {
                longest = (inode, prefix.clone(), index + 1);
            }
        }
        longest
    }
}

fn validate_dirent_type(ino: u64, ftype: Option<u8>, inode_is_dir: bool) -> io::Result<()> {
    let Some(ftype) = ftype.filter(|value| *value != XFS_DIR3_FT_UNKNOWN) else {
        return Ok(());
    };
    let dirent_is_dir = ftype == XFS_DIR3_FT_DIR;
    if dirent_is_dir != inode_is_dir {
        return Err(invalid_fs_data(format!(
            "directory entry type for inode {ino} conflicts with inode mode"
        )));
    }
    Ok(())
}

fn dirent_is_dir(ftype: Option<u8>) -> Option<bool> {
    ftype
        .filter(|value| *value != XFS_DIR3_FT_UNKNOWN)
        .map(|value| value == XFS_DIR3_FT_DIR)
}

fn is_plausible_shortform_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.contains('\0')
        && !matches!(name, "." | "..")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_graphic() || byte == b' ')
}
