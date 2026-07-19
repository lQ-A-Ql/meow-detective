use crate::{
    codec::{decode_string, CephDecode, CephStructEnvelope, CephUtime},
    CephCursor, CephWireError, Result,
};

use super::layout::{decode_file_layout, CephFsFileLayout};

pub const CEPH_FS_ONDISK_MAGIC: &str = "ceph fs volume v011";
const INODE_DECODER_VERSION: u8 = 20;
const INODE_COMPAT_VERSION: u8 = 6;
const INODE_STORE_DECODER_VERSION: u8 = 6;
const INODE_STORE_COMPAT_VERSION: u8 = 4;
pub const S_IFMT: u32 = 0o170000;
pub const S_IFREG: u32 = 0o100000;
pub const S_IFDIR: u32 = 0o040000;
pub const S_IFLNK: u32 = 0o120000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CephFsInodeKind {
    File,
    Directory,
    Symlink,
    Other,
}

impl CephFsInodeKind {
    fn from_mode(mode: u32) -> Self {
        match mode & S_IFMT {
            S_IFREG => Self::File,
            S_IFDIR => Self::Directory,
            S_IFLNK => Self::Symlink,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsInodeProjection {
    pub ino: u64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub nlink: i32,
    pub size: u64,
    pub kind: CephFsInodeKind,
    pub layout: CephFsFileLayout,
    pub encoded_version: u8,
    pub remaining_inode_bytes: usize,
}

impl CephFsInodeProjection {
    pub fn is_directory(&self) -> bool {
        self.kind == CephFsInodeKind::Directory
    }

    pub fn is_file(&self) -> bool {
        self.kind == CephFsInodeKind::File
    }
}

pub fn decode_cephfs_inode_object(input: &[u8]) -> Result<CephFsInodeProjection> {
    let mut cursor = CephCursor::new(input);
    let magic = decode_string(&mut cursor, 64, "CephFS inode magic")?;
    if magic != CEPH_FS_ONDISK_MAGIC {
        return Err(CephWireError::InvalidCephFsInode {
            field: "magic",
            reason: "inode object magic does not identify a CephFS inode",
        });
    }
    let projection = decode_cephfs_inode_store_cursor(&mut cursor)?;
    if !cursor.is_empty() {
        return Err(CephWireError::CephFsTrailingBytes {
            map: "inode_object",
            remaining: cursor.remaining(),
        });
    }
    Ok(projection)
}

pub fn decode_cephfs_inode_store(input: &[u8]) -> Result<CephFsInodeProjection> {
    let mut cursor = CephCursor::new(input);
    let projection = decode_cephfs_inode_store_cursor(&mut cursor)?;
    if !cursor.is_empty() {
        return Err(CephWireError::CephFsTrailingBytes {
            map: "inode_store",
            remaining: cursor.remaining(),
        });
    }
    Ok(projection)
}

pub fn decode_cephfs_inode_t_prefix(input: &[u8]) -> Result<CephFsInodeProjection> {
    let mut cursor = CephCursor::new(input);
    decode_inode_t_prefix_cursor(&mut cursor)
}

pub(crate) fn decode_cephfs_inode_store_cursor(
    cursor: &mut CephCursor<'_>,
) -> Result<CephFsInodeProjection> {
    let (envelope, mut payload) =
        CephStructEnvelope::decode_payload(cursor, INODE_STORE_COMPAT_VERSION)?;
    if envelope.version == 0
        || envelope.version > INODE_STORE_DECODER_VERSION
        || envelope.compat_version > INODE_STORE_COMPAT_VERSION
    {
        return Err(CephWireError::UnsupportedCephFsInodeVersion {
            structure: "InodeStore",
            encoded_version: envelope.version,
            compat_version: envelope.compat_version,
        });
    }
    decode_inode_t_prefix_cursor(&mut payload)
}

pub(crate) fn decode_inode_t_prefix_cursor(
    cursor: &mut CephCursor<'_>,
) -> Result<CephFsInodeProjection> {
    let (envelope, mut payload) = CephStructEnvelope::decode_payload(cursor, INODE_COMPAT_VERSION)?;
    if envelope.version == 0
        || envelope.version > INODE_DECODER_VERSION
        || envelope.compat_version > INODE_COMPAT_VERSION
    {
        return Err(CephWireError::UnsupportedCephFsInodeVersion {
            structure: "inode_t",
            encoded_version: envelope.version,
            compat_version: envelope.compat_version,
        });
    }
    let ino = u64::decode(&mut payload)?;
    if ino == 0 {
        return Err(CephWireError::InvalidCephFsInode {
            field: "ino",
            reason: "must be non-zero",
        });
    }
    u32::decode(&mut payload)?; // rdev
    CephUtime::decode(&mut payload)?; // ctime
    let mode = u32::decode(&mut payload)?;
    let uid = u32::decode(&mut payload)?;
    let gid = u32::decode(&mut payload)?;
    let nlink = i32::decode(&mut payload)?;
    let anchored = u8::decode(&mut payload)?;
    if anchored > 1 {
        return Err(CephWireError::InvalidCephFsInode {
            field: "anchored",
            reason: "boolean wire value is not zero or one",
        });
    }
    payload.skip(8)?; // ceph_dir_layout
    let layout = decode_file_layout(&mut payload)?;
    let size = u64::decode(&mut payload)?;
    Ok(CephFsInodeProjection {
        ino,
        mode,
        uid,
        gid,
        nlink,
        size,
        kind: CephFsInodeKind::from_mode(mode),
        layout,
        encoded_version: envelope.version,
        remaining_inode_bytes: payload.remaining(),
    })
}
