use crate::XfsReader;
use evidence_core::filesystem::invalid_fs_data;
use std::io::{self, Read, Seek, SeekFrom};

impl XfsReader {
    pub(crate) fn block_to_offset(&self, block: u64) -> io::Result<u64> {
        let byte_delta = block.checked_mul(self.block_size).ok_or_else(|| {
            invalid_fs_data(format!(
                "filesystem block {block} byte offset overflows (block_size={})",
                self.block_size
            ))
        })?;
        self.volume_offset.checked_add(byte_delta).ok_or_else(|| {
            invalid_fs_data(format!(
                "filesystem block {block} byte offset overflows (volume_offset={} block_size={})",
                self.volume_offset, self.block_size
            ))
        })
    }

    pub(crate) fn fsblock_to_linear_block(&self, fsb: u64) -> io::Result<u64> {
        if self.agblklog == 0 {
            if fsb >= self.dblocks {
                return Err(invalid_fs_data(format!(
                    "filesystem block {fsb} outside XFS data blocks (dblocks={})",
                    self.dblocks
                )));
            }
            return Ok(fsb);
        }
        if self.agblklog >= u64::BITS as u8 {
            return Err(invalid_fs_data(format!(
                "invalid XFS sb_agblklog {}",
                self.agblklog
            )));
        }

        let (ag_num, ag_block) = self.fsblock_parts(fsb)?;
        let linear = ag_num
            .checked_mul(self._ag_blocks)
            .and_then(|base| base.checked_add(ag_block))
            .ok_or_else(|| invalid_fs_data(format!("filesystem block {fsb} offset overflows")))?;
        if linear >= self.dblocks {
            return Err(invalid_fs_data(format!(
                "filesystem block {fsb} outside XFS data blocks (agno={ag_num} agbno={ag_block} linear={linear} dblocks={} agcount={} agblocks={})",
                self.dblocks, self._ag_count, self._ag_blocks
            )));
        }
        Ok(linear)
    }

    pub(crate) fn fsblock_to_offset(&self, fsb: u64) -> io::Result<u64> {
        self.block_to_offset(self.fsblock_to_linear_block(fsb)?)
    }

    pub(crate) fn add_fsblocks_within_ag(
        &self,
        start_fsb: u64,
        relative_fsb: u64,
    ) -> io::Result<u64> {
        if self.agblklog == 0 {
            let fsb = start_fsb.checked_add(relative_fsb).ok_or_else(|| {
                invalid_fs_data(format!(
                    "filesystem block addition overflows (start_fsb={start_fsb} relative_fsb={relative_fsb})"
                ))
            })?;
            if fsb >= self.dblocks {
                return Err(invalid_fs_data(format!(
                    "filesystem block {fsb} outside XFS data blocks (dblocks={})",
                    self.dblocks
                )));
            }
            return Ok(fsb);
        }

        let (ag_num, ag_block) = self.fsblock_parts(start_fsb)?;
        let new_ag_block = ag_block.checked_add(relative_fsb).ok_or_else(|| {
            invalid_fs_data(format!(
                "filesystem block addition overflows (start_fsb={start_fsb} relative_fsb={relative_fsb})"
            ))
        })?;
        if new_ag_block >= self._ag_blocks {
            return Err(invalid_fs_data(format!(
                "filesystem block range crosses XFS AG boundary (start_fsb={start_fsb} agno={ag_num} agbno={ag_block} relative_fsb={relative_fsb} agblocks={})",
                self._ag_blocks
            )));
        }
        self.compose_fsblock(ag_num, new_ag_block)
    }

    fn fsblock_parts(&self, fsb: u64) -> io::Result<(u64, u64)> {
        if self.agblklog >= u64::BITS as u8 {
            return Err(invalid_fs_data(format!(
                "invalid XFS sb_agblklog {}",
                self.agblklog
            )));
        }
        let ag_num = fsb >> self.agblklog;
        let ag_block = fsb & ((1u64 << self.agblklog) - 1);
        if ag_num >= u64::from(self._ag_count) || ag_block >= self._ag_blocks {
            return Err(invalid_fs_data(format!(
                "filesystem block {fsb} outside XFS AG geometry (agno={ag_num} agbno={ag_block} agcount={} agblocks={})",
                self._ag_count, self._ag_blocks
            )));
        }
        Ok((ag_num, ag_block))
    }

    fn compose_fsblock(&self, ag_num: u64, ag_block: u64) -> io::Result<u64> {
        let linear = ag_num
            .checked_mul(self._ag_blocks)
            .and_then(|base| base.checked_add(ag_block))
            .ok_or_else(|| invalid_fs_data("XFS AG block composition overflows"))?;
        if linear >= self.dblocks {
            return Err(invalid_fs_data(format!(
                "filesystem block outside XFS data blocks (agno={ag_num} agbno={ag_block} linear={linear} dblocks={})",
                self.dblocks
            )));
        }
        ag_num
            .checked_shl(u32::from(self.agblklog))
            .and_then(|encoded_ag| encoded_ag.checked_add(ag_block))
            .ok_or_else(|| invalid_fs_data("XFS encoded filesystem block overflows"))
    }

    pub(crate) fn read_block(&self, block: u64) -> io::Result<Vec<u8>> {
        let offset = self.fsblock_to_offset(block)?;
        let length = usize::try_from(self.block_size)
            .map_err(|_| invalid_fs_data("XFS block size exceeds addressable memory"))?;
        self.read_bytes_at(offset, length)
    }

    pub(crate) fn read_bytes_at(&self, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        let mut buf = vec![0u8; length];
        if length == 0 {
            return Ok(buf);
        }
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(offset))?;
        reader.read_exact(&mut buf)?;
        Ok(buf)
    }
}
