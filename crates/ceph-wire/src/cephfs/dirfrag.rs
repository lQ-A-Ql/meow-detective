use crate::{
    codec::{decode_string, CephDecode, CephStructEnvelope},
    CephCursor, CephWireError, Result,
};

use super::inode::{
    decode_cephfs_inode_store_cursor, decode_inode_t_prefix_cursor, CephFsInodeProjection,
};

pub const CEPH_NOSNAP: u64 = u64::MAX;
const MAX_DENTRY_NAME_BYTES: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CephFsDirfragIdentity {
    pub inode: u64,
    pub fragment: u32,
}

impl CephFsDirfragIdentity {
    pub fn new(inode: u64, fragment: u32) -> Result<Self> {
        if inode == 0 {
            return Err(CephWireError::InvalidCephFsDentry {
                context: "dirfrag",
                reason: "parent inode must be non-zero",
            });
        }
        Ok(Self { inode, fragment })
    }

    pub fn parse_object_name(value: &str) -> Result<Self> {
        let mut parts = value.split('.');
        let inode = parse_hex_u64(parts.next(), false)?;
        let fragment = parse_hex_u32(parts.next())?;
        if parts.next().is_some() {
            return Err(CephWireError::InvalidCephFsDentry {
                context: "dirfrag object name",
                reason: "object name has more than two components",
            });
        }
        Self::new(inode, fragment)
    }

    pub fn object_name(&self) -> String {
        format!("{:x}.{:08x}", self.inode, self.fragment)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CephFsDentryKey {
    pub name: String,
    pub snap_id: u64,
}

impl CephFsDentryKey {
    pub fn is_head(&self) -> bool {
        self.snap_id == CEPH_NOSNAP
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CephFsDentryKind {
    Primary,
    Remote { d_type: u8 },
}

impl CephFsDentryKind {
    pub fn is_directory_hint(&self) -> bool {
        matches!(self, Self::Remote { d_type: 4 })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsDentryProjection {
    pub key: CephFsDentryKey,
    pub first_snap: u64,
    pub kind: CephFsDentryKind,
    pub child_inode: u64,
    pub alternate_name: String,
    pub inode: Option<CephFsInodeProjection>,
}

pub fn decode_cephfs_dentry_key(input: &[u8]) -> Result<CephFsDentryKey> {
    let key = std::str::from_utf8(input).map_err(|_| CephWireError::InvalidCephFsDentry {
        context: "dentry key",
        reason: "key is not UTF-8",
    })?;
    let separator = key.rfind('_').ok_or(CephWireError::InvalidCephFsDentry {
        context: "dentry key",
        reason: "key has no snapshot suffix",
    })?;
    let name = &key[..separator];
    let suffix = &key[separator + 1..];
    if name.is_empty()
        || name.len() > MAX_DENTRY_NAME_BYTES
        || name.contains('\0')
        || name == "."
        || name == ".."
        || name.contains('/')
    {
        return Err(CephWireError::InvalidCephFsDentry {
            context: "dentry key",
            reason: "name is empty, unsafe, or exceeds the CephFS limit",
        });
    }
    let snap_id = if suffix == "head" {
        CEPH_NOSNAP
    } else {
        parse_hex_u64(Some(suffix), true)?
    };
    Ok(CephFsDentryKey {
        name: name.to_string(),
        snap_id,
    })
}

pub fn decode_cephfs_dentry_value(
    key: CephFsDentryKey,
    input: &[u8],
) -> Result<CephFsDentryProjection> {
    let mut cursor = CephCursor::new(input);
    let first_snap = u64::decode(&mut cursor)?;
    let dentry_type = u8::decode(&mut cursor)?;
    let (kind, child_inode, alternate_name, inode) = match dentry_type {
        b'I' => {
            let inode = decode_inode_t_prefix_cursor(&mut cursor)?;
            (
                CephFsDentryKind::Primary,
                inode.ino,
                String::new(),
                Some(inode),
            )
        }
        b'i' => {
            let (envelope, mut payload) = decode_dentry_envelope(&mut cursor)?;
            let alternate_name =
                decode_string(&mut payload, MAX_DENTRY_NAME_BYTES, "CephFS alternate name")?;
            let inode = decode_cephfs_inode_store_cursor(&mut payload)?;
            if envelope.version <= 2 && !payload.is_empty() {
                return Err(CephWireError::CephFsTrailingBytes {
                    map: "dentry_primary",
                    remaining: payload.remaining(),
                });
            }
            (
                CephFsDentryKind::Primary,
                inode.ino,
                alternate_name,
                Some(inode),
            )
        }
        b'L' => {
            let child_inode = u64::decode(&mut cursor)?;
            let d_type = u8::decode(&mut cursor)?;
            (
                CephFsDentryKind::Remote { d_type },
                child_inode,
                String::new(),
                None,
            )
        }
        b'l' => {
            let (envelope, mut payload) = decode_dentry_envelope(&mut cursor)?;
            let child_inode = u64::decode(&mut payload)?;
            let d_type = u8::decode(&mut payload)?;
            let alternate_name =
                decode_string(&mut payload, MAX_DENTRY_NAME_BYTES, "CephFS alternate name")?;
            if envelope.version <= 2 && !payload.is_empty() {
                return Err(CephWireError::CephFsTrailingBytes {
                    map: "dentry_remote",
                    remaining: payload.remaining(),
                });
            }
            (
                CephFsDentryKind::Remote { d_type },
                child_inode,
                alternate_name,
                None,
            )
        }
        value => return Err(CephWireError::UnsupportedCephFsDentryType { value }),
    };
    if child_inode == 0 {
        return Err(CephWireError::InvalidCephFsDentry {
            context: "dentry value",
            reason: "child inode must be non-zero",
        });
    }
    if let Some(inode) = &inode {
        if inode.ino != child_inode {
            return Err(CephWireError::InvalidCephFsDentry {
                context: "dentry value",
                reason: "embedded inode does not match dentry child inode",
            });
        }
    }
    if !cursor.is_empty() {
        return Err(CephWireError::CephFsTrailingBytes {
            map: "dentry_value",
            remaining: cursor.remaining(),
        });
    }
    Ok(CephFsDentryProjection {
        key,
        first_snap,
        kind,
        child_inode,
        alternate_name,
        inode,
    })
}

fn decode_dentry_envelope<'a>(
    cursor: &mut CephCursor<'a>,
) -> Result<(CephStructEnvelope, CephCursor<'a>)> {
    let envelope = CephStructEnvelope::decode(cursor)?;
    if !(1..=2).contains(&envelope.version) || envelope.compat_version > 1 {
        return Err(CephWireError::UnsupportedCephFsInodeVersion {
            structure: "dentry",
            encoded_version: envelope.version,
            compat_version: envelope.compat_version,
        });
    }
    let payload = cursor.take(envelope.payload_length as usize)?;
    Ok((envelope, payload))
}

fn parse_hex_u64(value: Option<&str>, allow_zero: bool) -> Result<u64> {
    let value = value.ok_or(CephWireError::InvalidCephFsDentry {
        context: "hex value",
        reason: "value is missing",
    })?;
    if value.is_empty()
        || (!allow_zero && value == "0")
        || (value.len() > 1 && value.starts_with('0'))
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(CephWireError::InvalidCephFsDentry {
            context: "hex value",
            reason: "value is not canonical lowercase hexadecimal",
        });
    }
    u64::from_str_radix(value, 16).map_err(|_| CephWireError::InvalidCephFsDentry {
        context: "hex value",
        reason: "value overflows u64",
    })
}

fn parse_hex_u32(value: Option<&str>) -> Result<u32> {
    let value = value.ok_or(CephWireError::InvalidCephFsDentry {
        context: "dirfrag fragment",
        reason: "fragment is missing",
    })?;
    if value.len() != 8
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(CephWireError::InvalidCephFsDentry {
            context: "dirfrag fragment",
            reason: "fragment is not eight lowercase hexadecimal digits",
        });
    }
    u32::from_str_radix(value, 16).map_err(|_| CephWireError::InvalidCephFsDentry {
        context: "dirfrag fragment",
        reason: "fragment overflows u32",
    })
}
