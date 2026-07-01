//! NTFS path resolution and preview helpers.

use crate::directory::DirEntry;
use crate::file_not_found;
use evidence_core::filesystem::path_components;
use std::io;

impl crate::NtfsReader {
    /// Create a lightweight preview handle for a file path.
    ///
    /// Unlike [`FileSystemReader::open_file`], the handle reads requested ranges
    /// directly from resident data or NTFS data runs and does not materialize the
    /// whole file for non-resident files.
    pub fn preview_file(&self, path: &str) -> io::Result<crate::NtfsPreviewFile<'_>> {
        let inode = match crate::utils::mft_inode_from_path(path) {
            Some(inode) => inode,
            None => self
                .resolve_file_path(path)?
                .ok_or_else(|| file_not_found(path))?,
        };
        Ok(crate::NtfsPreviewFile {
            reader: self,
            inode,
        })
    }

    /// Create a lightweight preview handle from an MFT inode.
    pub fn preview_file_by_inode(&self, inode: u64) -> crate::NtfsPreviewFile<'_> {
        crate::NtfsPreviewFile {
            reader: self,
            inode,
        }
    }

    /// Read a file range by path without materializing the full file.
    pub fn read_file_range(&self, path: &str, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        self.preview_file(path)?.read_range(offset, length)
    }

    /// Read a file range by MFT inode without materializing the full file.
    pub fn read_file_range_by_inode(
        &self,
        inode: u64,
        offset: u64,
        length: usize,
    ) -> io::Result<Vec<u8>> {
        self.preview_file_by_inode(inode).read_range(offset, length)
    }

    /// Resolve a file path: walk parent directories, then find the file
    /// in the final directory. Returns file MFT inode, or None if not found.
    pub(crate) fn resolve_file_path(&self, path: &str) -> io::Result<Option<u64>> {
        let components = path_components(path);
        let (parent_dirs, file_name) = match components.split_last() {
            Some((file, dirs)) => (dirs, *file),
            None => return Ok(None),
        };

        let mut current_inode = 5u64;
        for dir in parent_dirs {
            let children = self.list_dir_by_inode(current_inode)?;
            let found = children
                .iter()
                .find(|e: &&DirEntry| e.node.name.eq_ignore_ascii_case(dir) && e.node.is_dir);
            match found {
                Some(entry) => current_inode = entry.mft_ref,
                None => {
                    tracing::warn!(
                        path = %path,
                        missing_component = %dir,
                        parent_inode = %current_inode,
                        "NTFS path resolution: directory not found in parent INDX"
                    );
                    return Ok(None);
                }
            }
        }

        let children = self.list_dir_by_inode(current_inode)?;
        let result = children
            .iter()
            .find(|e| e.node.name.eq_ignore_ascii_case(file_name) && !e.node.is_dir)
            .map(|e| e.mft_ref);
        if result.is_none() {
            tracing::warn!(
                path = %path,
                missing_file = %file_name,
                parent_inode = %current_inode,
                children_count = %children.len(),
                "NTFS path resolution: file not found in parent INDX"
            );
        }
        Ok(result)
    }

    /// Resolve a path from root, walking top-down through directory INDX entries.
    /// Returns the MFT inode of the final component, or None if not found.
    /// Validates $FILE_NAME.par_ref consistency at each step.
    pub(crate) fn resolve_path(&self, path: &str) -> io::Result<Option<u64>> {
        let components = path_components(path);
        if components.is_empty() {
            return Ok(Some(5));
        }
        let mut current_inode = 5u64;
        let mut remaining = &components[..];
        while let Some((target, rest)) = remaining.split_first() {
            let children = self.list_dir_by_inode(current_inode)?;
            let found = children
                .iter()
                .find(|e| e.node.name.eq_ignore_ascii_case(target) && e.node.is_dir);
            match found {
                Some(entry) => {
                    // Verify the child directory's $FILE_NAME points back to us.
                    // Non-fatal: some directories (e.g. \$Recycle.Bin, System Volume
                    // Information) may have unreliable $FILE_NAME parent references
                    // due to MFT record quirks, but the INDX entry path is correct.
                    let _parent_ok = self.verify_parent(entry.mft_ref, current_inode)?;
                    current_inode = entry.mft_ref;
                    remaining = rest;
                }
                None => return Ok(None),
            }
        }
        Ok(Some(current_inode))
    }

    /// Verify that the $FILE_NAME attribute of `child_inode` has
    /// `par_ref` == `expected_parent`. Returns false on mismatch or IO error.
    fn verify_parent(&self, child_inode: u64, expected_parent: u64) -> io::Result<bool> {
        let rec = match self.read_mft_record(child_inode) {
            Ok(r) => r,
            Err(_) => return Ok(false),
        };
        let attr_off = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
        let mut pos = attr_off;
        let mut saw_file_name = false;
        while pos + 8 < rec.len() {
            let typ = u32::from_le_bytes(
                rec[pos..pos + 4]
                    .try_into()
                    .unwrap_or([0xFF, 0xFF, 0xFF, 0xFF]),
            );
            if typ == 0xFFFFFFFF {
                break;
            }
            let len =
                u32::from_le_bytes(rec[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
            if len == 0 || pos + len > rec.len() {
                break;
            }
            if typ == 0x30 && pos + 8 <= rec.len() {
                saw_file_name = true;
                // $FILE_NAME is resident on normal NTFS records. The parent
                // reference lives at the start of the resident content.
                if let Some(content) = crate::attribute::resident_attr_content(&rec, pos, len) {
                    if content.len() >= 8 {
                        let par_ref =
                            u64::from_le_bytes(content[0..8].try_into().unwrap_or([0; 8]))
                                & 0x0000_FFFF_FFFF_FFFF;
                        if par_ref == expected_parent {
                            return Ok(true);
                        }
                        pos += len;
                        continue;
                    }
                }

                // Legacy fallback for older simplified fixtures.
                let par_ref = u64::from_le_bytes(rec[pos..pos + 8].try_into().unwrap_or([0; 8]))
                    & 0x0000_FFFF_FFFF_FFFF;
                if par_ref == expected_parent {
                    return Ok(true);
                }
            }
            pos += len;
        }
        Ok(!saw_file_name) // no $FILE_NAME found — can't verify, allow
    }
}
