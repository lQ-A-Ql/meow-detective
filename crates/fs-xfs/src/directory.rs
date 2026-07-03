use crate::{
    be_u16, be_u32, be_u64, di_off, XfsExtent, XfsReader, BMBT_REC_SIZE, FORMAT_BTREE,
    FORMAT_EXTENTS, FORMAT_LOCAL,
};
use evidence_core::filesystem::{fs_out_of_memory, invalid_fs_data, path_components};
use std::io::{self, Read, Seek};

pub(crate) const DIR2_SF_HDR_8: usize = 10;
const DIR2_SF_HDR_4: usize = 6;

pub(crate) const XFS_DIR3_BLOCK_MAGIC: u32 = 0x5844_4233;
pub(crate) const XFS_DIR2_BLOCK_MAGIC: u32 = 0x5844_3242;
const XFS_DIR2_BLOCK_MAGIC_LEGACY: u32 = 0x5844_4232;
pub(crate) const XFS_DIR3_DATA_MAGIC: u32 = 0x5844_4433;
pub(crate) const XFS_DIR2_DATA_MAGIC: u32 = 0x5844_3244;
const XFS_DIR2_DATA_MAGIC_LEGACY: u32 = 0x5844_4432;
pub(crate) const XFS_DIR3_DATA_HDR_SIZE: usize = 64;
pub(crate) const XFS_DIR2_DATA_HDR_SIZE: usize = 16;
pub(crate) const XFS_DIR2_FREE_TAG: u16 = 0xFFFF;
const XFS_DIR2_SPACE_SIZE: u64 = 1u64 << (32 + 3);
const XFS_DIR2_DATA_SPACE: u64 = 0;
const XFS_DIR2_LEAF_SPACE: u64 = 1;

const XFS_DIR3_FT_UNKNOWN: u8 = 0;
#[cfg(test)]
pub(crate) const XFS_DIR3_FT_REG_FILE: u8 = 1;
pub(crate) const XFS_DIR3_FT_DIR: u8 = 2;
const XFS_DIR3_FT_MAX: u8 = 9;
pub(crate) const XFS_DIR2_DATA_ALIGN: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct XfsDirectoryEntry {
    pub(crate) name: String,
    pub(crate) inode: u64,
    pub(crate) ftype: Option<u8>,
}

pub(crate) struct XfsResolvedDirectoryEntry {
    pub(crate) name: String,
    pub(crate) inode: u64,
    pub(crate) is_dir: bool,
    pub(crate) size: u64,
}

#[derive(Default)]
struct DirectoryReadOutcome {
    entries: Vec<XfsDirectoryEntry>,
    first_error: Option<io::Error>,
    saw_partial_block: bool,
    saw_recoverable_block: bool,
}

impl DirectoryReadOutcome {
    fn record_error(&mut self, error: io::Error) {
        if self.first_error.is_none() {
            self.first_error = Some(error);
        }
    }

    fn should_try_residual_shortform(&self) -> bool {
        self.saw_partial_block || self.saw_recoverable_block
    }

    fn into_result(self) -> io::Result<Vec<XfsDirectoryEntry>> {
        match (self.entries.is_empty(), self.first_error) {
            (false, _) => Ok(self.entries),
            (true, Some(err)) => Err(err),
            (true, None) => Ok(self.entries),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirectoryBlockKind {
    Block { hdr_size: usize },
    Data { hdr_size: usize },
    Zero,
    Unknown(u32),
}

#[derive(Default)]
struct DirectoryBlockParse {
    entries: Vec<XfsDirectoryEntry>,
    error: Option<io::Error>,
    saw_recoverable_block: bool,
}

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
            .and_then(|len| usize::try_from(len).ok())
            .ok_or_else(|| fs_out_of_memory("xfs directory block exceeds addressable memory"))?;
        let offset = self.fsblock_to_offset(start_fsb)?;
        self.read_bytes_at(offset, byte_len)
    }

    fn read_directory_block_lossy(
        &self,
        start_fsb: u64,
        fsblock_count: u64,
    ) -> io::Result<(Vec<u8>, bool)> {
        let byte_len = fsblock_count
            .checked_mul(self.block_size)
            .and_then(|len| usize::try_from(len).ok())
            .ok_or_else(|| fs_out_of_memory("xfs directory block exceeds addressable memory"))?;
        let offset = self.fsblock_to_offset(start_fsb)?;
        self.read_bytes_at_lossy_zero_filled(offset, byte_len)
    }

    fn read_bytes_at_lossy_zero_filled(
        &self,
        offset: u64,
        length: usize,
    ) -> io::Result<(Vec<u8>, bool)> {
        let mut buf = vec![0u8; length];
        if length == 0 {
            return Ok((buf, false));
        }
        let mut reader = self.reader.borrow_mut();
        reader.seek(std::io::SeekFrom::Start(offset))?;
        let mut filled = 0usize;
        while filled < buf.len() {
            match reader.read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(error) => return Err(error),
            }
        }
        Ok((buf, filled < length))
    }

    fn parse_shortform_dir(data_fork: &[u8], has_ftype: bool) -> io::Result<Vec<(String, u64)>> {
        let min_hdr = DIR2_SF_HDR_4;
        if data_fork.len() < min_hdr {
            return Err(invalid_fs_data("shortform dir too small for header"));
        }
        let count = data_fork[0] as usize;
        let i8count = data_fork[1] as usize;

        let hdr_size = if i8count == 0 {
            DIR2_SF_HDR_4
        } else {
            DIR2_SF_HDR_8
        };
        if data_fork.len() < hdr_size {
            return Err(invalid_fs_data("shortform dir header truncated"));
        }

        let mut pos = hdr_size;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            if pos + 3 > data_fork.len() {
                break;
            }
            let namelen = data_fork[pos] as usize;
            let name_start = pos + 3;
            let Some(name_end) = name_start.checked_add(namelen) else {
                break;
            };
            if namelen == 0 || name_end > data_fork.len() {
                break;
            }

            let (inode_val, tail_len) = if has_ftype {
                if name_end >= data_fork.len() {
                    break;
                }
                let ftype = data_fork[name_end];
                if ftype >= XFS_DIR3_FT_MAX {
                    break;
                }
                let inode_off = name_end + 1;
                if i8count != 0 {
                    if inode_off + 8 > data_fork.len() {
                        break;
                    }
                    (be_u64(data_fork, inode_off) & 0x00FF_FFFF_FFFF_FFFF, 9)
                } else {
                    if inode_off + 4 > data_fork.len() {
                        break;
                    }
                    (be_u32(data_fork, inode_off) as u64, 5)
                }
            } else if i8count != 0 {
                if name_end + 8 > data_fork.len() {
                    break;
                }
                (be_u64(data_fork, name_end) & 0x00FF_FFFF_FFFF_FFFF, 8)
            } else {
                if name_end + 4 > data_fork.len() {
                    break;
                }
                (be_u32(data_fork, name_end) as u64, 4)
            };
            let name = String::from_utf8_lossy(&data_fork[name_start..name_end]).to_string();
            entries.push((name, inode_val));
            pos = name_end + tail_len;
        }
        Ok(entries)
    }

    fn read_extent_directory_entries(&self, inode: &[u8]) -> io::Result<Vec<XfsDirectoryEntry>> {
        let extents = Self::inline_extents(inode)?;
        let outcome = self.read_directory_entries_from_extents(&extents);
        self.directory_entries_from_outcome_raw(inode, outcome)
    }

    fn inline_extents(inode: &[u8]) -> io::Result<Vec<XfsExtent>> {
        let df = Self::data_fork(inode)?;
        let max_extents = Self::max_inline_extents(inode);
        let nextents = Self::nextents(inode) as usize;
        let mut extents = Vec::with_capacity(nextents.min(max_extents));

        for i in 0..nextents.min(max_extents) {
            let off = i * BMBT_REC_SIZE;
            if off + BMBT_REC_SIZE > df.len() {
                break;
            }
            extents.push(Self::decode_extent(&df[off..]));
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
            match self.read_directory_block_lossy(extent.start_block + relative_fsb, read_fsblocks)
            {
                Ok((block_data, is_partial)) => {
                    if is_partial {
                        outcome.saw_partial_block = true;
                    }
                    let mut parse = self.parse_block_dir_entries_lossy(&block_data);
                    outcome.saw_recoverable_block |= parse.saw_recoverable_block;
                    outcome.entries.append(&mut parse.entries);
                    if let Some(error) = parse.error {
                        outcome.record_error(error);
                    }
                }
                Err(err) => outcome.record_error(err),
            }
            relative_fsb = relative_fsb.saturating_add(read_fsblocks);
        }
    }

    fn directory_entries_from_outcome_raw(
        &self,
        inode: &[u8],
        mut outcome: DirectoryReadOutcome,
    ) -> io::Result<Vec<XfsDirectoryEntry>> {
        if outcome.should_try_residual_shortform() {
            let df = Self::data_fork(inode)?;
            let core = Self::inode_core_size(inode);
            let full_literal = inode.get(core..).unwrap_or_default();
            if let Some(recovered) =
                self.recover_shortform_dir_entries_raw(&[df, full_literal], self.has_ftype)
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
        }
        outcome.into_result()
    }

    fn extent_directory_data_is_all_zero(&self, inode: &[u8]) -> io::Result<bool> {
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
                let directory_bytes = logical_fsb.saturating_mul(self.block_size);
                if !Self::is_directory_data_space(directory_bytes) {
                    relative_fsb = relative_fsb.saturating_add(step);
                    continue;
                }

                let remaining = extent.block_count.saturating_sub(relative_fsb);
                let read_fsblocks = remaining.min(step);
                let block_data =
                    self.read_directory_block(extent.start_block + relative_fsb, read_fsblocks)?;
                saw_block = true;
                if block_data.iter().any(|&byte| byte != 0) {
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

    fn read_btree_directory_entries(&self, inode: &[u8]) -> io::Result<Vec<XfsDirectoryEntry>> {
        let extents = self.collect_btree_extents(inode)?;
        let outcome = self.read_directory_entries_from_extents(&extents);
        self.directory_entries_from_outcome_raw(inode, outcome)
    }

    #[cfg(test)]
    pub(crate) fn parse_block_dir(data: &[u8]) -> io::Result<Vec<(String, u64, bool)>> {
        Ok(Self::parse_block_dir_entries(data)?
            .into_iter()
            .map(|entry| {
                (
                    entry.name,
                    entry.inode,
                    entry.ftype == Some(XFS_DIR3_FT_DIR),
                )
            })
            .collect())
    }

    #[cfg(test)]
    pub(crate) fn parse_block_dir_entries(data: &[u8]) -> io::Result<Vec<XfsDirectoryEntry>> {
        let parse = Self::parse_block_dir_entries_impl_auto(data, false);
        if let Some(error) = parse.error {
            Err(error)
        } else {
            Ok(parse.entries)
        }
    }

    fn parse_block_dir_entries_lossy(&self, data: &[u8]) -> DirectoryBlockParse {
        Self::parse_block_dir_entries_impl(data, true, self.has_ftype)
    }

    #[cfg(test)]
    fn parse_block_dir_entries_impl_auto(
        data: &[u8],
        recoverable_magic: bool,
    ) -> DirectoryBlockParse {
        let parse_with_ftype = Self::parse_block_dir_entries_impl(data, recoverable_magic, true);
        if !parse_with_ftype.entries.is_empty() || parse_with_ftype.error.is_some() {
            return parse_with_ftype;
        }
        Self::parse_block_dir_entries_impl(data, recoverable_magic, false)
    }

    fn parse_block_dir_entries_impl(
        data: &[u8],
        recoverable_magic: bool,
        has_ftype: bool,
    ) -> DirectoryBlockParse {
        if data.len() < 8 {
            if data.is_empty() {
                return DirectoryBlockParse::default();
            }
            return DirectoryBlockParse {
                error: Some(invalid_fs_data("block directory buffer too short")),
                ..DirectoryBlockParse::default()
            };
        }
        let (hdr_size, has_block_tail) = match Self::classify_directory_block(data) {
            DirectoryBlockKind::Block { hdr_size } => (hdr_size, true),
            DirectoryBlockKind::Data { hdr_size } => (hdr_size, false),
            DirectoryBlockKind::Zero => {
                if !recoverable_magic {
                    return DirectoryBlockParse {
                        error: Some(invalid_fs_data("zeroed block directory data")),
                        saw_recoverable_block: true,
                        ..DirectoryBlockParse::default()
                    };
                }
                return DirectoryBlockParse {
                    saw_recoverable_block: true,
                    ..DirectoryBlockParse::default()
                };
            }
            DirectoryBlockKind::Unknown(magic) => {
                let error =
                    invalid_fs_data(format!("unknown block directory magic 0x{:08X}", magic));
                if recoverable_magic {
                    return DirectoryBlockParse {
                        error: Some(error),
                        saw_recoverable_block: true,
                        ..DirectoryBlockParse::default()
                    };
                }
                return DirectoryBlockParse {
                    error: Some(error),
                    ..DirectoryBlockParse::default()
                };
            }
        };
        if data.len() <= hdr_size {
            return DirectoryBlockParse::default();
        }

        let data_end = if has_block_tail && data.len() >= 8 {
            let leaf_count = be_u32(data, data.len() - 8) as usize;
            data.len()
                .saturating_sub(8)
                .saturating_sub(leaf_count * 8)
                .max(hdr_size)
        } else {
            data.len()
        };

        let mut pos = hdr_size;
        let mut entries = Vec::new();
        while pos + 11 <= data_end {
            let freetag = u16::from_be_bytes([data[pos], data[pos + 1]]);
            if freetag == XFS_DIR2_FREE_TAG {
                if pos + 4 > data.len() {
                    break;
                }
                let skip_len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
                if skip_len < 4 || pos + skip_len > data_end {
                    break;
                }
                pos = pos.saturating_add(skip_len.max(4));
                continue;
            }

            let inumber = be_u64(data, pos);
            let namelen = data[pos + 8] as usize;
            if inumber == 0 && namelen == 0 {
                break;
            }
            let name_start = pos + 9;
            if namelen == 0 {
                pos = pos.saturating_add(16);
                continue;
            }
            let entry_body_end = name_start + namelen;
            if entry_body_end > data_end {
                break;
            }
            let Some((ftype, padded_end)) =
                decode_dir_entry_tail(data, entry_body_end, pos, data_end, has_ftype)
            else {
                break;
            };
            let name = String::from_utf8_lossy(&data[name_start..entry_body_end]).to_string();

            if padded_end <= pos {
                break;
            }
            pos = padded_end;

            if inumber != 0 {
                if !is_plausible_directory_entry_name(&name) {
                    continue;
                }
                if entries
                    .iter()
                    .any(|entry: &XfsDirectoryEntry| entry.name == name)
                {
                    continue;
                }
                entries.push(XfsDirectoryEntry {
                    name,
                    inode: inumber,
                    ftype,
                });
            }
        }
        DirectoryBlockParse {
            entries,
            ..DirectoryBlockParse::default()
        }
    }

    fn classify_directory_block(data: &[u8]) -> DirectoryBlockKind {
        if data.len() < 4 || data.iter().all(|&byte| byte == 0) {
            return DirectoryBlockKind::Zero;
        }
        match be_u32(data, 0) {
            XFS_DIR3_BLOCK_MAGIC => DirectoryBlockKind::Block {
                hdr_size: XFS_DIR3_DATA_HDR_SIZE,
            },
            XFS_DIR2_BLOCK_MAGIC | XFS_DIR2_BLOCK_MAGIC_LEGACY => DirectoryBlockKind::Block {
                hdr_size: XFS_DIR2_DATA_HDR_SIZE,
            },
            XFS_DIR3_DATA_MAGIC => DirectoryBlockKind::Data {
                hdr_size: XFS_DIR3_DATA_HDR_SIZE,
            },
            XFS_DIR2_DATA_MAGIC | XFS_DIR2_DATA_MAGIC_LEGACY => DirectoryBlockKind::Data {
                hdr_size: XFS_DIR2_DATA_HDR_SIZE,
            },
            magic => DirectoryBlockKind::Unknown(magic),
        }
    }

    pub(crate) fn read_directory_entries(
        &self,
        ino: u64,
    ) -> io::Result<Vec<XfsResolvedDirectoryEntry>> {
        let inode = self.read_inode(ino)?;
        Self::validate_inode_magic(&inode)?;

        if !Self::inode_is_dir(&inode) {
            return Err(invalid_fs_data(format!("inode {} is not a directory", ino)));
        }

        let format = inode[di_off::FORMAT];
        match format {
            FORMAT_LOCAL => {
                let df = Self::data_fork(&inode)?;
                let raw = Self::parse_shortform_dir(df, self.has_ftype)?;
                Ok(self.annotate_shortform_entries(raw))
            }
            FORMAT_EXTENTS => match self.read_extent_directory_entries(&inode) {
                Ok(entries) => Ok(self.annotate_directory_entries(entries)),
                Err(block_err) => {
                    let all_zero = self
                        .extent_directory_data_is_all_zero(&inode)
                        .unwrap_or(false);
                    if all_zero {
                        let df = Self::data_fork(&inode)?;
                        let core = Self::inode_core_size(&inode);
                        let full_literal = &inode[core..];
                        if let Some(entries) =
                            self.recover_shortform_dir_entries(&[df, full_literal], self.has_ftype)
                        {
                            return Ok(entries);
                        }
                        Err(invalid_fs_data(
                            "block dir all-zero (sf->block conversion artifact), recovery failed",
                        ))
                    } else {
                        Err(block_err)
                    }
                }
            },
            FORMAT_BTREE => self
                .read_btree_directory_entries(&inode)
                .map(|entries| self.annotate_directory_entries(entries)),
            other => Err(invalid_fs_data(format!(
                "directory inode {} uses unsupported format {}",
                ino, other
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

    fn recover_shortform_dir_entries_raw(
        &self,
        slices: &[&[u8]],
        prefer_ftype: bool,
    ) -> Option<Vec<XfsDirectoryEntry>> {
        for &slice in slices {
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
            let count = slice[start] as usize;
            if count == 0 || count > 128 {
                continue;
            }
            let i8count = slice[start + 1] as usize;
            if i8count > count {
                continue;
            }
            let raw = match Self::parse_shortform_dir(&slice[start..], has_ftype) {
                Ok(raw) if raw.len() == count => raw,
                _ => continue,
            };
            if raw
                .iter()
                .all(|(name, ino)| is_plausible_shortform_name(name) && *ino > 0)
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
        let ftypes = Self::parse_shortform_dir_ftypes(data, has_ftype).unwrap_or_default();
        raw.into_iter()
            .enumerate()
            .map(|(idx, (name, inode))| XfsDirectoryEntry {
                name,
                inode,
                ftype: ftypes.get(idx).copied().flatten(),
            })
            .collect()
    }

    fn parse_shortform_dir_ftypes(
        data_fork: &[u8],
        has_ftype: bool,
    ) -> io::Result<Vec<Option<u8>>> {
        let min_hdr = DIR2_SF_HDR_4;
        if data_fork.len() < min_hdr {
            return Err(invalid_fs_data("shortform dir too small for header"));
        }
        let count = data_fork[0] as usize;
        let i8count = data_fork[1] as usize;
        let hdr_size = if i8count == 0 {
            DIR2_SF_HDR_4
        } else {
            DIR2_SF_HDR_8
        };
        if data_fork.len() < hdr_size {
            return Err(invalid_fs_data("shortform dir header truncated"));
        }

        let mut pos = hdr_size;
        let mut ftypes = Vec::with_capacity(count);
        for _ in 0..count {
            if pos + 3 > data_fork.len() {
                break;
            }
            let namelen = data_fork[pos] as usize;
            let name_end = pos + 3 + namelen;
            if namelen == 0 || name_end > data_fork.len() {
                break;
            }
            let inode_len = if i8count != 0 { 8 } else { 4 };
            let ftype = if has_ftype {
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
            ftypes.push(ftype);
            pos = name_end + inode_len + usize::from(has_ftype);
        }
        Ok(ftypes)
    }

    fn annotate_shortform_entries(
        &self,
        raw: Vec<(String, u64)>,
    ) -> Vec<XfsResolvedDirectoryEntry> {
        let mut entries = Vec::with_capacity(raw.len());
        for (name, child_ino) in raw {
            let metadata = self.child_inode_metadata(child_ino);
            entries.push(XfsResolvedDirectoryEntry {
                name,
                inode: child_ino,
                is_dir: metadata.map(|item| item.0).unwrap_or(false),
                size: metadata.map(|item| item.1).unwrap_or(0),
            });
        }
        entries
    }

    fn annotate_directory_entries(
        &self,
        raw: Vec<XfsDirectoryEntry>,
    ) -> Vec<XfsResolvedDirectoryEntry> {
        raw.into_iter()
            .map(|entry| {
                let metadata = self.child_inode_metadata(entry.inode);
                let is_dir = entry.ftype.map_or_else(
                    || metadata.map(|(is_dir, _)| is_dir).unwrap_or(false),
                    |ftype| {
                        if ftype != XFS_DIR3_FT_DIR {
                            return false;
                        }
                        metadata.map(|(is_dir, _)| is_dir).unwrap_or(true)
                    },
                );
                XfsResolvedDirectoryEntry {
                    name: entry.name,
                    inode: entry.inode,
                    is_dir,
                    size: metadata
                        .filter(|(metadata_is_dir, _)| !metadata_is_dir)
                        .map(|(_, size)| size)
                        .unwrap_or(0),
                }
            })
            .collect()
    }

    fn child_inode_metadata(&self, ino: u64) -> Option<(bool, u64)> {
        let inode = self.read_inode(ino).ok()?;
        Self::validate_inode_magic(&inode).ok()?;
        let is_dir = Self::inode_is_dir(&inode);
        if is_dir {
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

    pub(crate) fn resolve_path(&self, path: &str) -> io::Result<Option<(u64, bool)>> {
        let components = path_components(path);
        if components.is_empty() {
            return Ok(Some((self.root_ino, true)));
        }

        let mut current_ino = self.root_ino;
        for (i, component) in components.iter().enumerate() {
            let entries = self.read_directory_entries(current_ino)?;
            let is_last = i == components.len() - 1;
            let found = entries.iter().find(|entry| entry.name == *component);
            match found {
                Some(entry) => {
                    if is_last {
                        return Ok(Some((entry.inode, entry.is_dir)));
                    }
                    if !entry.is_dir {
                        return Ok(None);
                    }
                    current_ino = entry.inode;
                }
                None => return Ok(None),
            }
        }
        Ok(None)
    }
}

fn decode_dir_entry_tail(
    data: &[u8],
    name_end: usize,
    entry_start: usize,
    data_end: usize,
    has_ftype: bool,
) -> Option<(Option<u8>, usize)> {
    let raw_end = name_end.checked_add(if has_ftype { 3 } else { 2 })?;
    let padded_end = align_up(raw_end, XFS_DIR2_DATA_ALIGN)?;
    if padded_end > data_end || padded_end < 2 {
        return None;
    }

    let tag_pos = padded_end - 2;
    if be_u16(data, tag_pos) as usize != entry_start {
        return None;
    }

    let ftype = if has_ftype {
        let value = data.get(name_end)?;
        if *value >= XFS_DIR3_FT_MAX {
            return None;
        }
        if *value == XFS_DIR3_FT_UNKNOWN {
            None
        } else {
            Some(*value)
        }
    } else {
        None
    };

    Some((ftype, padded_end))
}

fn align_up(value: usize, align: usize) -> Option<usize> {
    if align == 0 {
        return Some(value);
    }
    let add = align.checked_sub(1)?;
    value.checked_add(add).map(|v| v & !add)
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

fn is_plausible_directory_entry_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.contains('\0')
        && !matches!(name, "." | "..")
        && name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b' ' | b'.'
                        | b'_'
                        | b'-'
                        | b'+'
                        | b','
                        | b'@'
                        | b'='
                        | b':'
                        | b'['
                        | b']'
                        | b'('
                        | b')'
                )
        })
}
