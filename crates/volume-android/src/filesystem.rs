use std::fmt;
use std::io::{Read, Seek, SeekFrom};

use crate::{Result, VolumeAndroidError};

const SUPERBLOCK_OFFSET: u64 = 1024;
const EXT4_MAGIC_OFFSET: u64 = SUPERBLOCK_OFFSET + 0x38;
const EXT4_MAGIC: u16 = 0xef53;
const F2FS_MAGIC: u32 = 0xf2f5_2010;
const EROFS_MAGIC: u32 = 0xe0f5_e1e2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AndroidFilesystemKind {
    Ext4,
    F2fs,
    Erofs,
    Unknown,
}

impl AndroidFilesystemKind {
    pub const fn has_reader(self) -> bool {
        matches!(self, Self::Ext4 | Self::F2fs)
    }

    pub fn require_reader(self) -> Result<Self> {
        match self {
            Self::Ext4 | Self::F2fs => Ok(self),
            Self::Erofs => Err(VolumeAndroidError::UnsupportedFilesystem { filesystem: self }),
            Self::Unknown => Err(VolumeAndroidError::UnrecognizedFilesystem),
        }
    }
}

impl fmt::Display for AndroidFilesystemKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Ext4 => "ext4",
            Self::F2fs => "F2FS",
            Self::Erofs => "EROFS",
            Self::Unknown => "unknown",
        })
    }
}

pub fn probe_filesystem<R: Read + Seek>(source: &mut R) -> Result<AndroidFilesystemKind> {
    source.seek(SeekFrom::Start(SUPERBLOCK_OFFSET))?;
    let mut primary_magic = [0u8; 4];
    source.read_exact(&mut primary_magic)?;
    match u32::from_le_bytes(primary_magic) {
        F2FS_MAGIC => return Ok(AndroidFilesystemKind::F2fs),
        EROFS_MAGIC => return Ok(AndroidFilesystemKind::Erofs),
        _ => {}
    }

    source.seek(SeekFrom::Start(EXT4_MAGIC_OFFSET))?;
    let mut ext4_magic = [0u8; 2];
    source.read_exact(&mut ext4_magic)?;
    if u16::from_le_bytes(ext4_magic) == EXT4_MAGIC {
        return Ok(AndroidFilesystemKind::Ext4);
    }
    Ok(AndroidFilesystemKind::Unknown)
}
