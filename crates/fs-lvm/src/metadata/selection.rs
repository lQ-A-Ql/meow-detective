use std::collections::BTreeMap;

use super::VolumeGroup;
use crate::error::{LvmError, Result};

pub fn select_committed_metadata<'a>(
    candidates: impl IntoIterator<Item = &'a VolumeGroup>,
) -> Result<Option<&'a VolumeGroup>> {
    let mut versions = BTreeMap::<(String, u64), &VolumeGroup>::new();
    let mut selected: Option<&VolumeGroup> = None;
    for candidate in candidates {
        let id = candidate
            .id
            .chars()
            .filter(|character| *character != '-')
            .collect::<String>()
            .to_ascii_lowercase();
        if id.is_empty() {
            return Err(LvmError::MetadataParseError {
                line: 0,
                message: "volume group UUID is missing".to_string(),
            });
        }
        let key = (id, candidate.seqno);
        if let Some(known) = versions.insert(key, candidate) {
            if known != candidate {
                return Err(LvmError::ConflictingMetadata {
                    vg_uuid: candidate.id.clone(),
                    seqno: candidate.seqno,
                });
            }
        }
        if selected.is_none_or(|current| candidate.seqno > current.seqno) {
            selected = Some(candidate);
        }
    }
    Ok(selected)
}
