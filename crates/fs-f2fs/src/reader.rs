use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use evidence_core::filesystem::path_components;
use evidence_core::EvidenceReader;

use crate::checkpoint::Checkpoint;
use crate::directory::{parse_directory_block, parse_inline_directory, DirectoryEntry};
use crate::file::F2fsFile;
use crate::inode::F2fsInode;
use crate::io::{block_offset, read_exact_at, SharedReader};
use crate::nat::NatTable;
use crate::{F2fsError, F2fsSuperblock, Result, F2FS_BLOCK_SIZE};

pub struct F2fsReader {
    pub(crate) source: SharedReader,
    pub(crate) volume_offset: u64,
    pub(crate) superblock: F2fsSuperblock,
    nat: NatTable,
    inode_cache: Mutex<HashMap<u32, Arc<F2fsInode>>>,
}

impl F2fsReader {
    pub fn open(source: Box<dyn EvidenceReader>, volume_offset: u64) -> Result<Self> {
        let source = Arc::new(Mutex::new(source));
        let superblock = F2fsSuperblock::read(&source, volume_offset)?;
        validate_source_size(&source, volume_offset, &superblock)?;
        let checkpoint = Checkpoint::read(&source, volume_offset, &superblock)?;
        let _checkpoint_version = checkpoint.version;
        let nat = NatTable::new(&superblock, &checkpoint);
        let reader = Self {
            source,
            volume_offset,
            superblock,
            nat,
            inode_cache: Mutex::new(HashMap::new()),
        };
        let root = reader.read_inode(reader.superblock.root_inode)?;
        if !root.is_directory() {
            return Err(F2fsError::Invalid(
                "F2FS root inode is not a directory".to_string(),
            ));
        }
        Ok(reader)
    }

    pub fn superblock(&self) -> &F2fsSuperblock {
        &self.superblock
    }

    pub(crate) fn read_inode(&self, nid: u32) -> Result<Arc<F2fsInode>> {
        if let Some(inode) = self
            .inode_cache
            .lock()
            .map_err(|_| F2fsError::Invalid("inode cache lock is poisoned".to_string()))?
            .get(&nid)
            .cloned()
        {
            return Ok(inode);
        }
        let nat = self.nat.lookup(&self.source, self.volume_offset, nid)?;
        if nat.inode != nid {
            return Err(F2fsError::Invalid(format!(
                "NAT entry {nid} belongs to inode {}",
                nat.inode
            )));
        }
        if nat.block < self.superblock.main_block
            || u64::from(nat.block) >= self.superblock.block_count
        {
            return Err(F2fsError::Invalid(format!(
                "inode {nid} node block {} is outside the main area",
                nat.block
            )));
        }
        let bytes = read_exact_at(
            &self.source,
            block_offset(self.volume_offset, nat.block)?,
            F2FS_BLOCK_SIZE,
        )?;
        let inode = Arc::new(F2fsInode::parse(&bytes, nid, nat.inode)?);
        self.inode_cache
            .lock()
            .map_err(|_| F2fsError::Invalid("inode cache lock is poisoned".to_string()))?
            .insert(nid, Arc::clone(&inode));
        Ok(inode)
    }

    pub(crate) fn resolve_path(&self, path: &str) -> Result<Arc<F2fsInode>> {
        let mut inode = self.read_inode(self.superblock.root_inode)?;
        for component in path_components(path) {
            let child = self
                .read_directory(&inode)?
                .into_iter()
                .find(|entry| entry.name == component)
                .ok_or_else(|| F2fsError::NotFound(path.to_string()))?;
            inode = self.read_inode(child.inode)?;
        }
        Ok(inode)
    }

    pub(crate) fn read_directory(&self, inode: &F2fsInode) -> Result<Vec<DirectoryEntry>> {
        if !inode.is_directory() {
            return Err(F2fsError::Invalid(format!(
                "inode {} is not a directory",
                inode.nid
            )));
        }
        inode.require_unencrypted("directory entries")?;
        if let Some(data) = inode.inline_directory_data() {
            return parse_inline_directory(data);
        }
        inode.require_external_data("directory entries")?;
        let block_count = inode.required_blocks()?;
        let mut entries = Vec::new();
        for block in inode.data_blocks.iter().take(block_count) {
            if *block == 0 {
                continue;
            }
            if *block < self.superblock.main_block
                || u64::from(*block) >= self.superblock.block_count
            {
                return Err(F2fsError::Invalid(format!(
                    "directory inode {} references invalid block {block}",
                    inode.nid
                )));
            }
            let bytes = read_exact_at(
                &self.source,
                block_offset(self.volume_offset, *block)?,
                F2FS_BLOCK_SIZE,
            )?;
            entries.extend(parse_directory_block(&bytes)?);
        }
        Ok(entries)
    }

    pub(crate) fn open_regular_file(&self, inode: &F2fsInode) -> Result<F2fsFile> {
        if inode.is_symlink() {
            return Err(F2fsError::Unsupported(format!(
                "symlink inode {}",
                inode.nid
            )));
        }
        if !inode.is_regular() {
            return Err(F2fsError::Invalid(format!(
                "inode {} is not a regular file",
                inode.nid
            )));
        }
        inode.require_unencrypted("file data")?;
        if let Some(data) = inode.inline_file_data() {
            return F2fsFile::from_inline(inode.size, data);
        }
        inode.require_external_data("file data")?;
        let blocks = inode.data_blocks[..inode.required_blocks()?].to_vec();
        F2fsFile::new(
            Arc::clone(&self.source),
            self.volume_offset,
            self.superblock.main_block,
            self.superblock.block_count,
            inode.size,
            blocks,
        )
    }
}

fn validate_source_size(
    source: &SharedReader,
    volume_offset: u64,
    superblock: &F2fsSuperblock,
) -> Result<()> {
    let expected_end = superblock
        .block_count
        .checked_mul(F2FS_BLOCK_SIZE as u64)
        .and_then(|size| volume_offset.checked_add(size))
        .ok_or_else(|| F2fsError::Invalid("filesystem size overflows".to_string()))?;
    let actual_size = source
        .lock()
        .map_err(|_| F2fsError::Invalid("evidence reader lock is poisoned".to_string()))?
        .info()
        .size;
    if expected_end > actual_size {
        return Err(F2fsError::Invalid(format!(
            "F2FS declares end offset {expected_end}, beyond source size {actual_size}"
        )));
    }
    Ok(())
}
