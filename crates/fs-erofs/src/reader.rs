use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

use evidence_core::EvidenceReader;

use crate::directory::{parse_directory_block, DirectoryEntry};
use crate::file::ErofsFile;
use crate::inode::ErofsInode;
use crate::io::{block_offset, read_exact_at, SharedReader};
use crate::{ErofsError, ErofsSuperblock, Result};

pub struct ErofsReader {
    pub(crate) source: SharedReader,
    pub(crate) volume_offset: u64,
    pub(crate) superblock: ErofsSuperblock,
    inode_cache: Mutex<HashMap<u64, Arc<ErofsInode>>>,
}

impl ErofsReader {
    pub fn open(source: Box<dyn EvidenceReader>, volume_offset: u64) -> Result<Self> {
        let source = Arc::new(Mutex::new(source));
        let superblock = ErofsSuperblock::read(&source, volume_offset)?;
        validate_source_size(&source, volume_offset, &superblock)?;
        let reader = Self {
            source,
            volume_offset,
            superblock,
            inode_cache: Mutex::new(HashMap::new()),
        };
        let root = reader.read_inode(reader.superblock.root_nid)?;
        if !root.is_directory() {
            return Err(ErofsError::Invalid(
                "EROFS root inode is not a directory".to_string(),
            ));
        }
        Ok(reader)
    }

    pub fn superblock(&self) -> &ErofsSuperblock {
        &self.superblock
    }

    pub(crate) fn read_inode(&self, nid: u64) -> Result<Arc<ErofsInode>> {
        if let Some(inode) = self
            .inode_cache
            .lock()
            .map_err(|_| ErofsError::Invalid("inode cache lock is poisoned".to_string()))?
            .get(&nid)
            .cloned()
        {
            return Ok(inode);
        }
        if nid >= (1u64 << 63) {
            return Err(ErofsError::Unsupported("metabox inode".to_string()));
        }
        let inode_table = block_offset(
            self.volume_offset,
            self.superblock.meta_block,
            self.superblock.block_size,
        )?;
        let offset = inode_table
            .checked_add(
                nid.checked_mul(32)
                    .ok_or_else(|| ErofsError::Invalid("inode offset overflows".to_string()))?,
            )
            .ok_or_else(|| ErofsError::Invalid("inode offset overflows".to_string()))?;
        let bytes = read_exact_at(&self.source, offset, 64)?;
        let inode = Arc::new(ErofsInode::parse(&bytes, nid, offset)?);
        self.inode_cache
            .lock()
            .map_err(|_| ErofsError::Invalid("inode cache lock is poisoned".to_string()))?
            .insert(nid, Arc::clone(&inode));
        Ok(inode)
    }

    pub(crate) fn resolve_path(&self, path: &str) -> Result<Arc<ErofsInode>> {
        let mut inode = self.read_inode(self.superblock.root_nid)?;
        for component in evidence_core::filesystem::path_components(path) {
            let child = self
                .read_directory(&inode)?
                .into_iter()
                .find(|entry| entry.name == component)
                .ok_or_else(|| ErofsError::NotFound(path.to_string()))?;
            inode = self.read_inode(child.nid)?;
        }
        Ok(inode)
    }

    pub(crate) fn read_directory(&self, inode: &ErofsInode) -> Result<Vec<DirectoryEntry>> {
        if !inode.is_directory() {
            return Err(ErofsError::Invalid(format!(
                "inode {} is not a directory",
                inode.nid
            )));
        }
        inode.require_uncompressed("directory entries")?;
        let mut file = self.open_inode_data(inode)?;
        let blocks = inode.size.div_ceil(self.superblock.block_size as u64);
        let mut entries = Vec::new();
        for index in 0..blocks {
            let remaining = inode.size - index * self.superblock.block_size as u64;
            let valid = usize::try_from(remaining)
                .unwrap_or(self.superblock.block_size)
                .min(self.superblock.block_size);
            let mut bytes = vec![0u8; self.superblock.block_size];
            file.seek(SeekFrom::Start(index * self.superblock.block_size as u64))?;
            file.read_exact(&mut bytes[..valid])?;
            entries.extend(parse_directory_block(&bytes, valid)?);
        }
        Ok(entries)
    }

    pub(crate) fn open_file_data(&self, inode: &ErofsInode) -> Result<ErofsFile> {
        if !inode.is_regular() && !inode.is_symlink() {
            return Err(ErofsError::Invalid(format!(
                "inode {} has no readable file data",
                inode.nid
            )));
        }
        inode.require_uncompressed("file data")?;
        self.open_inode_data(inode)
    }

    fn open_inode_data(&self, inode: &ErofsInode) -> Result<ErofsFile> {
        ErofsFile::new(
            Arc::clone(&self.source),
            self.volume_offset,
            self.superblock.block_size,
            self.superblock.block_count,
            inode.start_block,
            inode.inline_data_offset()?,
            inode.size,
        )
    }
}

fn validate_source_size(
    source: &SharedReader,
    volume_offset: u64,
    superblock: &ErofsSuperblock,
) -> Result<()> {
    let expected_end = superblock
        .block_count
        .checked_mul(superblock.block_size as u64)
        .and_then(|size| volume_offset.checked_add(size))
        .ok_or_else(|| ErofsError::Invalid("filesystem size overflows".to_string()))?;
    let actual_size = source
        .lock()
        .map_err(|_| ErofsError::Invalid("evidence reader lock is poisoned".to_string()))?
        .info()
        .size;
    if expected_end > actual_size {
        return Err(ErofsError::Invalid(format!(
            "EROFS declares end offset {expected_end}, beyond source size {actual_size}"
        )));
    }
    Ok(())
}
