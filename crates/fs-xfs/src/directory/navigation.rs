use super::{
    XfsDirectoryEntry, XfsResolvedDirectoryEntry, DIR2_SF_HDR_4, DIR2_SF_HDR_8, XFS_DIR3_FT_DIR,
    XFS_DIR3_FT_MAX,
};
use crate::{be_u64, di_off, XfsReader, FORMAT_BTREE, FORMAT_EXTENTS, FORMAT_LOCAL};
use evidence_core::filesystem::{invalid_fs_data, path_components};
use std::io;

const MAX_DIRECTORY_PATH_CACHE_ENTRIES: usize = 100_000;
const MAX_DIRECTORY_INODE_CACHE_ENTRIES: usize = 32_768;

impl XfsReader {
    pub(crate) fn cache_directory_path(&self, path: String, ino: u64) {
        let mut cache = self.directory_path_cache.borrow_mut();
        if cache.len() < MAX_DIRECTORY_PATH_CACHE_ENTRIES {
            cache.insert(path, ino);
        }
    }

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

        match inode[di_off::FORMAT] {
            FORMAT_LOCAL => {
                let raw = Self::parse_shortform_dir(Self::data_fork(&inode)?, self.has_ftype)?;
                Ok(self.annotate_shortform_entries(raw))
            }
            FORMAT_EXTENTS => match self.read_extent_directory_entries(&inode) {
                Ok(entries) => Ok(self.annotate_directory_entries(entries)),
                Err(block_error) => {
                    if !self
                        .extent_directory_data_is_all_zero(&inode)
                        .unwrap_or(false)
                    {
                        return Err(block_error);
                    }
                    let data_fork = Self::data_fork(&inode)?;
                    let full_literal = &inode[Self::inode_core_size(&inode)..];
                    self.recover_shortform_dir_entries(&[data_fork, full_literal], self.has_ftype)
                        .ok_or_else(|| {
                            invalid_fs_data(
                            "block dir all-zero (sf->block conversion artifact), recovery failed",
                        )
                        })
                }
            },
            FORMAT_BTREE => self
                .read_btree_directory_entries(&inode)
                .map(|entries| self.annotate_directory_entries(entries)),
            other => Err(invalid_fs_data(format!(
                "directory inode {ino} uses unsupported format {other}"
            ))),
        }
    }

    fn recover_shortform_dir_entries(
        &self,
        slices: &[&[u8]],
        prefer_ftype: bool,
    ) -> Option<Vec<XfsResolvedDirectoryEntry>> {
        self.recover_shortform_dir_entries_raw(slices, prefer_ftype)
            .map(|entries| self.annotate_directory_entries(entries))
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

    fn annotate_shortform_entries(
        &self,
        raw: Vec<(String, u64)>,
    ) -> Vec<XfsResolvedDirectoryEntry> {
        raw.into_iter()
            .map(|(name, inode)| {
                let metadata = self.child_inode_metadata(inode);
                XfsResolvedDirectoryEntry {
                    name,
                    inode,
                    is_dir: metadata.map(|item| item.0).unwrap_or(false),
                    size: metadata.map(|item| item.1).unwrap_or(0),
                }
            })
            .collect()
    }

    fn annotate_directory_entries(
        &self,
        raw: Vec<XfsDirectoryEntry>,
    ) -> Vec<XfsResolvedDirectoryEntry> {
        raw.into_iter()
            .map(|entry| {
                let metadata = self.child_inode_metadata(entry.inode);
                let is_dir = entry.ftype.map_or_else(
                    || metadata.map(|item| item.0).unwrap_or(false),
                    |file_type| {
                        file_type == XFS_DIR3_FT_DIR && metadata.map(|item| item.0).unwrap_or(true)
                    },
                );
                XfsResolvedDirectoryEntry {
                    name: entry.name,
                    inode: entry.inode,
                    is_dir,
                    size: metadata
                        .filter(|item| !item.0)
                        .map(|item| item.1)
                        .unwrap_or(0),
                }
            })
            .collect()
    }

    fn child_inode_metadata(&self, ino: u64) -> Option<(bool, u64)> {
        let inode = self.read_inode(ino).ok()?;
        Self::validate_inode_magic(&inode).ok()?;
        if Self::inode_is_dir(&inode) {
            let mut cache = self.directory_inode_cache.borrow_mut();
            if cache.len() < MAX_DIRECTORY_INODE_CACHE_ENTRIES {
                cache.entry(ino).or_insert_with(|| inode.clone());
            }
            return Some((
                matches!(
                    inode[di_off::FORMAT],
                    FORMAT_LOCAL | FORMAT_EXTENTS | FORMAT_BTREE
                ),
                0,
            ));
        }
        Some((false, be_u64(&inode, di_off::SIZE)))
    }

    fn lookup_directory_entry(&self, ino: u64, name: &str) -> io::Result<Option<(u64, bool)>> {
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

        let entries = match inode[di_off::FORMAT] {
            FORMAT_LOCAL => {
                let data_fork = Self::data_fork(&inode)?;
                let raw = Self::parse_shortform_dir(data_fork, self.has_ftype)?;
                Self::shortform_entries_to_directory_entries(raw, self.has_ftype, data_fork)
            }
            FORMAT_EXTENTS => match self.read_extent_directory_entries(&inode) {
                Ok(entries) => entries,
                Err(block_error) => {
                    if !self
                        .extent_directory_data_is_all_zero(&inode)
                        .unwrap_or(false)
                    {
                        return Err(block_error);
                    }
                    let data_fork = Self::data_fork(&inode)?;
                    let full_literal = &inode[Self::inode_core_size(&inode)..];
                    self.recover_shortform_dir_entries_raw(
                        &[data_fork, full_literal],
                        self.has_ftype,
                    )
                    .ok_or_else(|| {
                        invalid_fs_data(
                            "block dir all-zero (sf->block conversion artifact), recovery failed",
                        )
                    })?
                }
            },
            FORMAT_BTREE => self.read_btree_directory_entries(&inode)?,
            other => {
                return Err(invalid_fs_data(format!(
                    "directory inode {ino} uses unsupported format {other}"
                )))
            }
        };
        let Some(entry) = entries.into_iter().find(|entry| entry.name == name) else {
            return Ok(None);
        };
        let metadata = self.child_inode_metadata(entry.inode);
        let is_dir = entry.ftype.map_or_else(
            || metadata.map(|item| item.0).unwrap_or(false),
            |file_type| file_type == XFS_DIR3_FT_DIR && metadata.map(|item| item.0).unwrap_or(true),
        );
        Ok(Some((entry.inode, is_dir)))
    }

    pub(crate) fn resolve_path(&self, path: &str) -> io::Result<Option<(u64, bool)>> {
        let components = path_components(path);
        if components.is_empty() {
            return Ok(Some((self.root_ino, true)));
        }
        let normalized_path = components.join("/");
        if let Some(inode) = self
            .directory_path_cache
            .borrow()
            .get(&normalized_path)
            .copied()
        {
            return Ok(Some((inode, true)));
        }

        let mut current_inode = self.root_ino;
        let mut current_path = String::new();
        for (index, component) in components.iter().enumerate() {
            let is_last = index == components.len() - 1;
            let Some((entry_inode, entry_is_dir)) =
                self.lookup_directory_entry(current_inode, component)?
            else {
                return Ok(None);
            };
            current_path = if current_path.is_empty() {
                (*component).to_string()
            } else {
                format!("{current_path}/{component}")
            };
            if entry_is_dir {
                self.cache_directory_path(current_path.clone(), entry_inode);
            }
            if is_last {
                return Ok(Some((entry_inode, entry_is_dir)));
            }
            if !entry_is_dir {
                return Ok(None);
            }
            current_inode = entry_inode;
        }
        Ok(None)
    }
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
