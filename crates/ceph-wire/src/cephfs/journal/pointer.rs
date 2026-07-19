use crate::{
    codec::{CephDecode, CephStructEnvelope},
    cursor::CephCursor,
    error::{CephWireError, Result},
};

use super::CephFsJournalPointer;

pub fn decode_cephfs_journal_pointer(input: &[u8]) -> Result<CephFsJournalPointer> {
    let mut cursor = CephCursor::new(input);
    let (envelope, mut payload) = CephStructEnvelope::decode_payload(&mut cursor, 1)?;
    if envelope.version < 1 || envelope.compat_version > 1 {
        return Err(CephWireError::UnsupportedCephFsJournalVersion {
            structure: "pointer",
            encoded_version: envelope.version,
            compat_version: envelope.compat_version,
        });
    }
    let pointer = CephFsJournalPointer {
        front: u64::decode(&mut payload)?,
        back: u64::decode(&mut payload)?,
    };
    if !cursor.is_empty() {
        return Err(CephWireError::InvalidCephFsJournal {
            context: "pointer",
            reason: "trailing bytes after envelope",
        });
    }
    if pointer.front == 0 {
        return Err(CephWireError::InvalidCephFsJournal {
            context: "pointer",
            reason: "active journal inode is zero",
        });
    }
    Ok(pointer)
}
