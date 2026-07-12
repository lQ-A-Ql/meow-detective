use crate::format::{BTRFS_MAGIC, BTRFS_SUPERBLOCK_OFFSET, CHUNK_ITEM_KEY, KEY_SIZE};
use crate::types::BtrfsChunk;
use crate::BtrfsReader;
use evidence_core::filesystem::invalid_fs_data;
use evidence_core::EvidenceReader;
use std::cell::RefCell;
use std::io::{self, Read, Seek, SeekFrom};

impl BtrfsReader {
    pub fn open(mut reader: Box<dyn EvidenceReader>, offset: u64) -> io::Result<Self> {
        reader.seek(SeekFrom::Start(offset + BTRFS_SUPERBLOCK_OFFSET))?;
        let mut sb = [0u8; 4096];
        reader.read_exact(&mut sb)?;

        if &sb[0x40..0x48] != BTRFS_MAGIC {
            return Err(invalid_fs_data(format!(
                "not a valid btrfs filesystem (magic {:02X?})",
                &sb[0x40..0x48]
            )));
        }

        let sectorsize = u32::from_le_bytes(
            sb[0xB8..0xBC]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        );
        let nodesize = u32::from_le_bytes(
            sb[0xBC..0xC0]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        );
        let root_tree_logical = u64::from_le_bytes(
            sb[0x78..0x80]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        );
        let chunk_tree_logical = u64::from_le_bytes(
            sb[0x80..0x88]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        );
        if sectorsize == 0 || nodesize == 0 {
            return Err(invalid_fs_data("invalid btrfs geometry"));
        }

        let sys_chunk_array_size = u32::from_le_bytes(
            sb[0xC8..0xCC]
                .try_into()
                .map_err(|_| invalid_fs_data("disk parse error"))?,
        ) as usize;
        let sys_chunk_start = 0x32B;
        let sys_chunk_end = (sys_chunk_start + sys_chunk_array_size).min(sb.len());

        let mut reader_obj = Self {
            reader: RefCell::new(reader),
            _sectorsize: sectorsize,
            nodesize,
            root_tree_logical,
            chunk_tree_logical,
            volume_offset: offset,
            chunk_map: Vec::new(),
            subvolumes: Vec::new(),
            default_subvol_root_bytenr: 0,
            default_subvol_root_dirid: crate::format::FIRST_FREE_OBJECTID,
        };

        reader_obj.parse_chunks(&sb[sys_chunk_start..sys_chunk_end])?;
        if reader_obj.chunk_tree_logical != 0
            && reader_obj.chunk_tree_logical != reader_obj.root_tree_logical
        {
            let _ = reader_obj.read_chunk_tree();
        }
        reader_obj.discover_subvolumes()?;

        if reader_obj.default_subvol_root_bytenr == 0 {
            if let Some(default_sv) = reader_obj
                .subvolumes
                .iter()
                .find(|s| s.name == "default" || s.id == crate::format::FS_TREE_OBJECTID)
            {
                reader_obj.default_subvol_root_bytenr = default_sv.tree_root_bytenr;
                reader_obj.default_subvol_root_dirid = default_sv.root_dirid;
            } else if let Some(first) = reader_obj.subvolumes.first() {
                reader_obj.default_subvol_root_bytenr = first.tree_root_bytenr;
                reader_obj.default_subvol_root_dirid = first.root_dirid;
            }
        }
        Ok(reader_obj)
    }

    pub(crate) fn translate_logical(&self, logical: u64) -> io::Result<u64> {
        for chunk in &self.chunk_map {
            if logical >= chunk.logical && logical < chunk.logical + chunk.length {
                return Ok((logical - chunk.logical) + chunk.physical);
            }
        }
        Ok(logical)
    }

    pub(crate) fn parse_chunks(&mut self, data: &[u8]) -> io::Result<()> {
        let mut pos = 0usize;
        while pos + KEY_SIZE + 8 <= data.len() {
            let key = crate::types::BtrfsKey::parse(&data[pos..pos + KEY_SIZE])?;
            pos += KEY_SIZE;
            if key.ty != CHUNK_ITEM_KEY {
                break;
            }
            if pos + 0x30 > data.len() {
                break;
            }
            let length = u64::from_le_bytes(
                data[pos..pos + 8]
                    .try_into()
                    .map_err(|_| invalid_fs_data("disk parse error"))?,
            );
            let num_stripes = u16::from_le_bytes(
                data[pos + 0x2C..pos + 0x2E]
                    .try_into()
                    .map_err(|_| invalid_fs_data("disk parse error"))?,
            );
            if num_stripes > 0 {
                let stripe_offset = pos + 0x30;
                let phys = u64::from_le_bytes(
                    data[stripe_offset + 8..stripe_offset + 16]
                        .try_into()
                        .map_err(|_| invalid_fs_data("btrfs chunk stripe physical too short"))?,
                );
                self.chunk_map.push(BtrfsChunk {
                    logical: key.offset,
                    length,
                    physical: phys,
                });
            }
            pos += 0x30 + num_stripes as usize * 0x20;
        }
        Ok(())
    }

    pub(crate) fn read_logical_block(&self, logical: u64) -> io::Result<Vec<u8>> {
        self.read_logical_range(logical, self.nodesize as usize)
    }

    pub(crate) fn read_logical_range(&self, logical: u64, length: usize) -> io::Result<Vec<u8>> {
        let physical = self.translate_logical(logical)?;
        let absolute = self.volume_offset + physical;
        let mut buf = vec![0u8; length];
        if length == 0 {
            return Ok(buf);
        }
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(absolute))?;
        reader.read_exact(&mut buf)?;
        Ok(buf)
    }
}
