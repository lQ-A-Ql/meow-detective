//! Fail-closed planning for bounded in-place file rewrites.
//!
//! The planner never writes evidence. It resolves one regular file, proves
//! that its existing data fork is fully allocated and unshared, then returns
//! volume-relative patches for a caller-owned copy-on-write layer.

use crate::log::{assess_log_state, XfsLogState, XFS_LOG_MAX_SNAPSHOT_BYTES};
use crate::reader::S_IFMT;
use crate::{be_u16, be_u64, di_off, XfsExtent, XfsReader, FORMAT_BTREE, FORMAT_EXTENTS};
use evidence_core::filesystem::{file_not_found, invalid_fs_data, path_is_directory};
use std::io;

const XFS_DIFLAG_REALTIME: u16 = 1 << 0;
const XFS_DIFLAG2_NREXT64: u64 = 1 << 4;
const XFS_DI_FLAGS_OFFSET: usize = 0x5A;
const XFS_S_IFREG: u16 = 0x8000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XfsFileRewritePatch {
    pub volume_offset: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XfsFileRewritePlan {
    pub old_size: u64,
    pub patches: Vec<XfsFileRewritePatch>,
}

impl XfsReader {
    pub fn file_size_by_path(&self, path: &str) -> io::Result<u64> {
        let resolved = self
            .resolve_path_with_inode(path)?
            .ok_or_else(|| file_not_found(path))?;
        if resolved.is_dir {
            return Err(path_is_directory(path));
        }
        let inode = resolved
            .inode
            .map_or_else(|| self.read_inode(resolved.inode_number), Ok)?;
        Self::validate_inode_magic(&inode)?;
        Self::validate_inode_core_length(&inode)?;
        Ok(be_u64(&inode, di_off::SIZE))
    }

    /// Plan a non-growing rewrite over the file's existing allocation.
    ///
    /// Dirty or incomplete internal logs, sparse/unwritten/reflink/realtime
    /// layouts, local forks, malformed extent maps, and growth are rejected.
    pub fn plan_in_place_file_rewrite(
        &self,
        path: &str,
        content: &[u8],
    ) -> io::Result<XfsFileRewritePlan> {
        self.require_clean_log()?;
        if self.has_reflink {
            return Err(unsupported(
                "reflink-capable XFS volumes cannot be rewritten safely",
            ));
        }
        let (inode_number, mut inode, old_size, extents) = self.rewrite_target(path, content)?;
        let extents = self.validate_rewrite_extents(extents, old_size)?;
        let mut patches = self.build_data_patches(&extents, content, old_size)?;
        self.validate_inode_for_rewrite(inode_number, &inode)?;
        inode[di_off::SIZE..di_off::SIZE + 8]
            .copy_from_slice(&(content.len() as u64).to_be_bytes());
        Self::reseal_inode(&mut inode)?;
        patches.push(XfsFileRewritePatch {
            volume_offset: self.relative_offset(self.inode_offset(inode_number)?)?,
            bytes: inode,
        });
        Ok(XfsFileRewritePlan { old_size, patches })
    }

    fn require_clean_log(&self) -> io::Result<()> {
        let snapshot = self
            .read_internal_log_snapshot(XFS_LOG_MAX_SNAPSHOT_BYTES)
            .map_err(|error| invalid_fs_data(format!("cannot assess XFS log: {error}")))?;
        if !snapshot.complete {
            return Err(invalid_fs_data(
                "XFS internal log exceeds the complete assessment limit",
            ));
        }
        if assess_log_state(&snapshot) != XfsLogState::Clean {
            return Err(invalid_fs_data(
                "XFS internal log is dirty; repair it in the overlay before rewriting files",
            ));
        }
        Ok(())
    }

    fn rewrite_target(
        &self,
        path: &str,
        content: &[u8],
    ) -> io::Result<(u64, Vec<u8>, u64, Vec<XfsExtent>)> {
        let resolved = self
            .resolve_path_with_inode(path)?
            .ok_or_else(|| file_not_found(path))?;
        if resolved.is_dir {
            return Err(path_is_directory(path));
        }
        let inode = resolved
            .inode
            .map_or_else(|| self.read_inode(resolved.inode_number), Ok)?;
        Self::validate_inode_magic(&inode)?;
        Self::validate_inode_core_length(&inode)?;
        if be_u16(&inode, di_off::MODE) & S_IFMT != XFS_S_IFREG {
            return Err(unsupported("only regular XFS files can be rewritten"));
        }
        if be_u16(&inode, XFS_DI_FLAGS_OFFSET) & XFS_DIFLAG_REALTIME != 0 {
            return Err(unsupported("realtime XFS files cannot be rewritten"));
        }
        if inode.get(di_off::VERSION) == Some(&3)
            && be_u64(&inode, di_off::FLAGS2) & XFS_DIFLAG2_NREXT64 != 0
        {
            return Err(unsupported("XFS NREXT64 inode rewrites are not supported"));
        }
        let old_size = be_u64(&inode, di_off::SIZE);
        let new_size = u64::try_from(content.len())
            .map_err(|_| invalid_fs_data("replacement content length overflows u64"))?;
        if new_size > old_size {
            return Err(unsupported("replacement content cannot grow the XFS file"));
        }
        let extents = match Self::inode_format(&inode)? {
            FORMAT_EXTENTS => Self::inline_extents(&inode)?,
            FORMAT_BTREE => self.collect_btree_extents(&inode)?,
            _ => {
                return Err(unsupported(
                    "XFS file does not use an extent or BMBT data fork",
                ))
            }
        };
        if extents.len() != Self::nextents(&inode) as usize {
            return Err(invalid_fs_data(
                "XFS extent count does not match di_nextents",
            ));
        }
        Ok((resolved.inode_number, inode, old_size, extents))
    }

    fn validate_rewrite_extents(
        &self,
        mut extents: Vec<XfsExtent>,
        old_size: u64,
    ) -> io::Result<Vec<XfsExtent>> {
        extents.sort_by_key(|extent| extent.logical);
        let mut covered_until = 0u64;
        let mut physical_ranges = Vec::with_capacity(extents.len());
        for extent in &extents {
            if extent.unwritten || extent.block_count == 0 {
                return Err(unsupported(
                    "unwritten or empty XFS extents cannot be rewritten",
                ));
            }
            let start = extent
                .logical
                .checked_mul(self.block_size)
                .ok_or_else(|| invalid_fs_data("XFS logical extent offset overflows"))?;
            let length = extent
                .block_count
                .checked_mul(self.block_size)
                .ok_or_else(|| invalid_fs_data("XFS extent length overflows"))?;
            let end = start
                .checked_add(length)
                .ok_or_else(|| invalid_fs_data("XFS logical extent end overflows"))?;
            if start != covered_until {
                let kind = if start < covered_until {
                    "overlap"
                } else {
                    "gap"
                };
                return Err(unsupported(format!("XFS extent map contains a {kind}")));
            }
            let physical_start = self.fsblock_to_linear_block(extent.start_block)?;
            let last = self.add_fsblocks_within_ag(extent.start_block, extent.block_count - 1)?;
            let physical_end = self
                .fsblock_to_linear_block(last)?
                .checked_add(1)
                .ok_or_else(|| invalid_fs_data("XFS physical extent end overflows"))?;
            physical_ranges.push((physical_start, physical_end));
            covered_until = end;
        }
        physical_ranges.sort_unstable();
        if physical_ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
            return Err(unsupported("XFS extent map contains a physical overlap"));
        }
        if covered_until < old_size {
            return Err(unsupported(
                "XFS extent map does not cover the complete file",
            ));
        }
        Ok(extents)
    }

    fn build_data_patches(
        &self,
        extents: &[XfsExtent],
        content: &[u8],
        old_size: u64,
    ) -> io::Result<Vec<XfsFileRewritePatch>> {
        let new_size = content.len() as u64;
        let mut patches = Vec::new();
        for extent in extents {
            let logical_start = extent.logical * self.block_size;
            if logical_start >= old_size {
                break;
            }
            let logical_end = (logical_start + extent.block_count * self.block_size).min(old_size);
            let physical = self.relative_offset(self.fsblock_to_offset(extent.start_block)?)?;
            if logical_start < new_size {
                let end = logical_end.min(new_size) as usize;
                patches.push(XfsFileRewritePatch {
                    volume_offset: physical,
                    bytes: content[logical_start as usize..end].to_vec(),
                });
            }
            let zero_start = logical_start.max(new_size);
            if zero_start < logical_end {
                let delta = zero_start - logical_start;
                let volume_offset = physical
                    .checked_add(delta)
                    .ok_or_else(|| invalid_fs_data("XFS physical extent offset overflows"))?;
                patches.push(XfsFileRewritePatch {
                    volume_offset,
                    bytes: vec![0; (logical_end - zero_start) as usize],
                });
            }
        }
        Ok(patches)
    }

    fn validate_inode_for_rewrite(&self, inode_number: u64, inode: &[u8]) -> io::Result<()> {
        match (self.log_geometry.metadata_crc, inode[di_off::VERSION]) {
            (false, 1 | 2) => Ok(()),
            (true, 3) => {
                if inode.get(152..160) != Some(inode_number.to_be_bytes().as_slice())
                    || inode.get(160..176) != Some(self.metadata_uuid.as_slice())
                {
                    return Err(invalid_fs_data(
                        "XFS v3 inode identity does not match the volume",
                    ));
                }
                if !crate::log::replay::metadata_crc_is_valid(inode) {
                    return Err(invalid_fs_data(
                        "XFS v3 inode CRC is invalid before rewrite",
                    ));
                }
                Ok(())
            }
            (metadata_crc, version) => Err(unsupported(format!(
                "XFS inode version {version} is incompatible with metadata_crc={metadata_crc}"
            ))),
        }
    }

    fn reseal_inode(inode: &mut [u8]) -> io::Result<()> {
        if inode[di_off::VERSION] == 3 {
            crate::log::replay::stamp_metadata_crc(inode);
        }
        Ok(())
    }

    fn relative_offset(&self, absolute: u64) -> io::Result<u64> {
        absolute
            .checked_sub(self.volume_offset)
            .ok_or_else(|| invalid_fs_data("XFS patch offset precedes the volume"))
    }
}

fn unsupported(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, message.into())
}

#[cfg(test)]
#[path = "../tests/unit/rewrite.rs"]
mod tests;
