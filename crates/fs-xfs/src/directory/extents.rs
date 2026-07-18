use super::{
    DirectoryReadOutcome, XfsDirectoryEntry, XFS_DIR2_DATA_SPACE, XFS_DIR2_LEAF_SPACE,
    XFS_DIR2_SPACE_SIZE,
};
use crate::{XfsExtent, XfsReader, BMBT_REC_SIZE};
use evidence_core::filesystem::{
    fs_out_of_memory, invalid_fs_data, FileSystemDiagnostic, FileSystemDiagnosticKind,
};
use std::io;

impl XfsReader {
    pub(crate) fn directory_block_fsblocks(&self) -> io::Result<u64> {
        if self.dirblklog >= u64::BITS as u8 {
            return Err(invalid_fs_data(format!(
                "invalid XFS sb_dirblklog {}",
                self.dirblklog
            )));
        }
        Ok(1u64 << self.dirblklog)
    }

    fn read_directory_block(&self, start_fsb: u64, fsblock_count: u64) -> io::Result<Vec<u8>> {
        let byte_len = fsblock_count
            .checked_mul(self.block_size)
            .and_then(|length| usize::try_from(length).ok())
            .ok_or_else(|| fs_out_of_memory("xfs directory block exceeds addressable memory"))?;
        self.read_bytes_at(self.fsblock_to_offset(start_fsb)?, byte_len)
    }

    pub(super) fn read_extent_directory_entries(
        &self,
        inode: &[u8],
    ) -> io::Result<Vec<XfsDirectoryEntry>> {
        let outcome = self.read_directory_entries_from_extents(&Self::inline_extents(inode)?);
        self.directory_entries_from_outcome_raw(inode, outcome)
    }

    pub(crate) fn inline_extents(inode: &[u8]) -> io::Result<Vec<XfsExtent>> {
        let data_fork = Self::data_fork(inode)?;
        let max_extents = Self::max_inline_extents(inode);
        let nextents = Self::nextents(inode) as usize;
        let count = nextents.min(max_extents);
        let mut extents = Vec::with_capacity(count);
        for index in 0..count {
            let offset = index * BMBT_REC_SIZE;
            if offset + BMBT_REC_SIZE > data_fork.len() {
                break;
            }
            extents.push(Self::decode_extent(&data_fork[offset..]));
        }
        Ok(extents)
    }

    fn read_directory_entries_from_extents(&self, extents: &[XfsExtent]) -> DirectoryReadOutcome {
        let mut outcome = DirectoryReadOutcome::default();
        let dir_block_fsblocks = match self.directory_block_fsblocks() {
            Ok(value) => value,
            Err(error) => {
                outcome.record_error(error);
                return outcome;
            }
        };
        for extent in extents {
            self.read_directory_extent_blocks(*extent, dir_block_fsblocks, &mut outcome);
        }
        outcome
    }

    fn read_directory_extent_blocks(
        &self,
        extent: XfsExtent,
        dir_block_fsblocks: u64,
        outcome: &mut DirectoryReadOutcome,
    ) {
        if extent.unwritten {
            return;
        }
        let step = dir_block_fsblocks.max(1);
        let mut relative_fsb = 0u64;
        while relative_fsb < extent.block_count {
            let logical_fsb = extent.logical.saturating_add(relative_fsb);
            let directory_bytes = logical_fsb.saturating_mul(self.block_size);
            if !Self::is_directory_data_space(directory_bytes) {
                relative_fsb = relative_fsb.saturating_add(step);
                continue;
            }

            let remaining = extent.block_count.saturating_sub(relative_fsb);
            let read_fsblocks = remaining.min(step);
            match self
                .add_fsblocks_within_ag(extent.start_block, relative_fsb)
                .and_then(|start_fsb| self.read_directory_block(start_fsb, read_fsblocks))
            {
                Ok(block_data) => {
                    let mut parse = self.parse_block_dir_entries_lossy(&block_data);
                    outcome.saw_recoverable_block |= parse.saw_recoverable_block;
                    outcome.entries.append(&mut parse.entries);
                    if let Some(error) = parse.error {
                        outcome.record_error(error);
                    }
                }
                Err(error) => outcome.record_error(error),
            }
            relative_fsb = relative_fsb.saturating_add(read_fsblocks);
        }
    }

    fn directory_entries_from_outcome_raw(
        &self,
        inode: &[u8],
        mut outcome: DirectoryReadOutcome,
    ) -> io::Result<Vec<XfsDirectoryEntry>> {
        self.recover_residual_shortform(inode, &mut outcome)?;
        if !outcome.entries.is_empty() {
            if let Some(error) = outcome.first_error.as_ref() {
                self.record_diagnostic(FileSystemDiagnostic::new(
                    FileSystemDiagnosticKind::DirectoryPartial,
                    format!(
                        "XFS directory retained readable entries after a directory block failed: {error}"
                    ),
                ));
            }
        }
        outcome.into_result()
    }

    fn recover_residual_shortform(
        &self,
        inode: &[u8],
        outcome: &mut DirectoryReadOutcome,
    ) -> io::Result<()> {
        if !outcome.should_try_residual_shortform() {
            return Ok(());
        }
        let had_entries = !outcome.entries.is_empty();
        let data_fork = Self::data_fork(inode)?;
        let full_literal = inode
            .get(Self::inode_core_size(inode)..)
            .unwrap_or_default();
        if let Some(recovered) =
            self.recover_shortform_dir_entries_raw(&[data_fork, full_literal], self.has_ftype)
        {
            for entry in recovered {
                if !outcome
                    .entries
                    .iter()
                    .any(|existing| existing.name == entry.name)
                {
                    outcome.entries.push(entry);
                }
            }
        }
        if outcome.entries.is_empty() && !had_entries {
            return Err(outcome.first_error.take().unwrap_or_else(|| {
                invalid_fs_data(
                    "recoverable block directory data produced no entries and residual shortform recovery failed",
                )
            }));
        }
        Ok(())
    }

    pub(super) fn extent_directory_data_is_all_zero(&self, inode: &[u8]) -> io::Result<bool> {
        let extents = Self::inline_extents(inode)?;
        let dir_block_fsblocks = self.directory_block_fsblocks()?;
        let mut saw_block = false;
        for extent in extents {
            if extent.unwritten {
                continue;
            }
            let step = dir_block_fsblocks.max(1);
            let mut relative_fsb = 0u64;
            while relative_fsb < extent.block_count {
                let logical_fsb = extent.logical.saturating_add(relative_fsb);
                if !Self::is_directory_data_space(logical_fsb.saturating_mul(self.block_size)) {
                    relative_fsb = relative_fsb.saturating_add(step);
                    continue;
                }
                let read_fsblocks = extent.block_count.saturating_sub(relative_fsb).min(step);
                let start_fsb = self.add_fsblocks_within_ag(extent.start_block, relative_fsb)?;
                let block_data = self.read_directory_block(start_fsb, read_fsblocks)?;
                saw_block = true;
                if block_data.iter().any(|byte| *byte != 0) {
                    return Ok(false);
                }
                relative_fsb = relative_fsb.saturating_add(read_fsblocks);
            }
        }
        Ok(saw_block)
    }

    fn is_directory_data_space(directory_byte_offset: u64) -> bool {
        directory_byte_offset >= XFS_DIR2_DATA_SPACE.saturating_mul(XFS_DIR2_SPACE_SIZE)
            && directory_byte_offset < XFS_DIR2_LEAF_SPACE.saturating_mul(XFS_DIR2_SPACE_SIZE)
    }

    pub(super) fn read_btree_directory_entries(
        &self,
        inode: &[u8],
    ) -> io::Result<Vec<XfsDirectoryEntry>> {
        let outcome = self.read_directory_entries_from_extents(&self.collect_btree_extents(inode)?);
        self.directory_entries_from_outcome_raw(inode, outcome)
    }
}
