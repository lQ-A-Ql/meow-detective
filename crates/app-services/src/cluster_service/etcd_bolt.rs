use std::collections::BTreeSet;

use super::kubernetes_parser_error::{KubernetesParserError, Result};

const BOLT_MAGIC: u32 = 0xED0C_DAED;
const META_PAGE_COUNT: u64 = 2;
const MAX_ETCD_BYTES: usize = 512 * 1024 * 1024;
const MAX_ETCD_ENTRIES: usize = 131_072;
const MAX_ETCD_DEPTH: usize = 32;
const PAGE_HEADER_SIZE: usize = 16;
const LEAF_ELEMENT_SIZE: usize = 16;
const BRANCH_ELEMENT_SIZE: usize = 16;
const LEAF_FLAG: u16 = 0x02;
const BUCKET_LEAF_FLAG: u32 = 0x01;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EtcdBoltSummary {
    pub page_size: u32,
    pub txid: u64,
    pub root_page: u64,
    pub bucket_count: usize,
    pub key_count: usize,
    pub entries: Vec<EtcdBoltEntry>,
    pub checksum_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EtcdBoltEntry {
    pub bucket_path: String,
    pub key: String,
    pub value_length: usize,
    pub nested_bucket: bool,
    pub value_redacted: bool,
}

pub fn parse_etcd_bolt_metadata(bytes: &[u8]) -> Result<EtcdBoltSummary> {
    if bytes.len() > MAX_ETCD_BYTES {
        return Err(KubernetesParserError::Limit {
            kind: "etcd bolt bytes",
            actual: bytes.len(),
            max: MAX_ETCD_BYTES,
        });
    }
    if bytes.len() < 4096 * META_PAGE_COUNT as usize {
        return Err(KubernetesParserError::Truncated {
            offset: bytes.len() as u64,
            context: "etcd bolt meta pages",
        });
    }
    let first = parse_meta(bytes, 0, 4096).ok();
    let second_offset = first.as_ref().map(|meta| meta.0 as usize).unwrap_or(4096);
    let second = parse_meta(bytes, second_offset, 4096).ok();
    let (page_size, txid, root_page, checksum_verified) = match (first, second) {
        (Some(left), Some(right)) if right.1 >= left.1 => right,
        (Some(left), _) => left,
        (_, Some(right)) => right,
        (None, None) => {
            return Err(KubernetesParserError::InvalidBinary {
                offset: 0,
                reason: "no valid Bolt meta page".to_string(),
            })
        }
    };
    let page_size_usize = page_size as usize;
    let mut state = WalkState {
        bytes,
        page_size: page_size_usize,
        entries: Vec::new(),
        buckets: BTreeSet::new(),
        seen_pages: BTreeSet::new(),
    };
    if root_page != 0 {
        walk_page(root_page, 0, &mut state, String::new())?;
    }
    let key_count = state
        .entries
        .iter()
        .filter(|entry| !entry.nested_bucket)
        .count();
    Ok(EtcdBoltSummary {
        page_size,
        txid,
        root_page,
        bucket_count: state.buckets.len(),
        key_count,
        entries: state.entries,
        checksum_verified,
    })
}

struct WalkState<'a> {
    bytes: &'a [u8],
    page_size: usize,
    entries: Vec<EtcdBoltEntry>,
    buckets: BTreeSet<String>,
    seen_pages: BTreeSet<u64>,
}

fn parse_meta(
    bytes: &[u8],
    offset: usize,
    default_page_size: u32,
) -> Result<(u32, u64, u64, bool)> {
    if offset.checked_add(64).is_none_or(|end| end > bytes.len()) {
        return Err(KubernetesParserError::Truncated {
            offset: offset as u64,
            context: "Bolt meta",
        });
    }
    let magic = read_u32(bytes, offset)?;
    if magic != BOLT_MAGIC {
        return Err(KubernetesParserError::InvalidBinary {
            offset: offset as u64,
            reason: "invalid Bolt magic".to_string(),
        });
    }
    let page_size = read_u32(bytes, offset + 8)?;
    let page_size = if page_size == 0 {
        default_page_size
    } else {
        page_size
    };
    if !page_size.is_power_of_two() || !(512..=65_536).contains(&page_size) {
        return Err(KubernetesParserError::InvalidBinary {
            offset: (offset + 8) as u64,
            reason: format!("invalid Bolt page size {page_size}"),
        });
    }
    let root_page = read_u64(bytes, offset + 16)?;
    let txid = read_u64(bytes, offset + 48)?;
    Ok((page_size, txid, root_page, false))
}

fn walk_page(
    page_id: u64,
    depth: usize,
    state: &mut WalkState<'_>,
    bucket_path: String,
) -> Result<()> {
    if depth > MAX_ETCD_DEPTH {
        return Err(KubernetesParserError::Limit {
            kind: "etcd bucket depth",
            actual: depth,
            max: MAX_ETCD_DEPTH,
        });
    }
    if !state.seen_pages.insert(page_id) {
        return Ok(());
    }
    let page_offset = page_id.checked_mul(state.page_size as u64).ok_or_else(|| {
        KubernetesParserError::InvalidBinary {
            offset: page_id,
            reason: "page offset overflow".to_string(),
        }
    })? as usize;
    let header_end = page_offset.checked_add(PAGE_HEADER_SIZE).ok_or_else(|| {
        KubernetesParserError::InvalidBinary {
            offset: page_id,
            reason: "page header overflow".to_string(),
        }
    })?;
    if header_end > state.bytes.len() {
        return Err(KubernetesParserError::Truncated {
            offset: page_offset as u64,
            context: "Bolt page header",
        });
    }
    let flags = read_u16(state.bytes, page_offset + 8)?;
    let count = read_u16(state.bytes, page_offset + 10)? as usize;
    let overflow = read_u32(state.bytes, page_offset + 12)? as usize;
    let page_end = page_offset
        .checked_add(state.page_size.checked_mul(overflow + 1).ok_or_else(|| {
            KubernetesParserError::InvalidBinary {
                offset: page_offset as u64,
                reason: "page extent overflow".to_string(),
            }
        })?)
        .ok_or_else(|| KubernetesParserError::InvalidBinary {
            offset: page_offset as u64,
            reason: "page extent overflow".to_string(),
        })?;
    if page_end > state.bytes.len() {
        return Err(KubernetesParserError::Truncated {
            offset: page_offset as u64,
            context: "Bolt page extent",
        });
    }
    if flags & LEAF_FLAG != 0 {
        walk_leaf(page_offset, page_end, count, depth, state, bucket_path)
    } else if flags & 0x01 != 0 {
        walk_branch(page_offset, page_end, count, depth, state, bucket_path)
    } else {
        Ok(())
    }
}

fn walk_leaf(
    page_offset: usize,
    page_end: usize,
    count: usize,
    depth: usize,
    state: &mut WalkState<'_>,
    bucket_path: String,
) -> Result<()> {
    let elements_end = page_offset
        .checked_add(PAGE_HEADER_SIZE + count * LEAF_ELEMENT_SIZE)
        .ok_or_else(|| KubernetesParserError::InvalidBinary {
            offset: page_offset as u64,
            reason: "leaf element overflow".to_string(),
        })?;
    if elements_end > page_end {
        return Err(KubernetesParserError::Truncated {
            offset: elements_end as u64,
            context: "Bolt leaf elements",
        });
    }
    for index in 0..count {
        if state.entries.len() >= MAX_ETCD_ENTRIES {
            return Err(KubernetesParserError::Limit {
                kind: "etcd entries",
                actual: state.entries.len() + 1,
                max: MAX_ETCD_ENTRIES,
            });
        }
        let element = page_offset + PAGE_HEADER_SIZE + index * LEAF_ELEMENT_SIZE;
        let flags = read_u32(state.bytes, element)?;
        let position = read_u32(state.bytes, element + 4)? as usize;
        let key_size = read_u32(state.bytes, element + 8)? as usize;
        let value_size = read_u32(state.bytes, element + 12)? as usize;
        let key_start = page_offset.checked_add(position).ok_or_else(|| {
            KubernetesParserError::InvalidBinary {
                offset: element as u64,
                reason: "leaf key offset overflow".to_string(),
            }
        })?;
        let key_end = key_start.checked_add(key_size).ok_or_else(|| {
            KubernetesParserError::InvalidBinary {
                offset: element as u64,
                reason: "leaf key length overflow".to_string(),
            }
        })?;
        let value_end = key_end.checked_add(value_size).ok_or_else(|| {
            KubernetesParserError::InvalidBinary {
                offset: element as u64,
                reason: "leaf value length overflow".to_string(),
            }
        })?;
        if value_end > page_end {
            return Err(KubernetesParserError::Truncated {
                offset: key_start as u64,
                context: "Bolt leaf key/value",
            });
        }
        let key = display_key(&state.bytes[key_start..key_end]);
        let nested_bucket = flags & BUCKET_LEAF_FLAG != 0 && value_size >= 16;
        state.entries.push(EtcdBoltEntry {
            bucket_path: bucket_path.clone(),
            key: key.clone(),
            value_length: value_size,
            nested_bucket,
            value_redacted: value_size > 0,
        });
        if nested_bucket {
            let child_page = read_u64(state.bytes, key_end)?;
            let child_path = if bucket_path.is_empty() {
                key
            } else {
                format!("{bucket_path}/{key}")
            };
            state.buckets.insert(child_path.clone());
            if child_page != 0 {
                walk_page(child_page, depth + 1, state, child_path)?;
            }
        }
    }
    Ok(())
}

fn walk_branch(
    page_offset: usize,
    page_end: usize,
    count: usize,
    depth: usize,
    state: &mut WalkState<'_>,
    bucket_path: String,
) -> Result<()> {
    let elements_end = page_offset
        .checked_add(PAGE_HEADER_SIZE + count * BRANCH_ELEMENT_SIZE)
        .ok_or_else(|| KubernetesParserError::InvalidBinary {
            offset: page_offset as u64,
            reason: "branch element overflow".to_string(),
        })?;
    if elements_end > page_end {
        return Err(KubernetesParserError::Truncated {
            offset: elements_end as u64,
            context: "Bolt branch elements",
        });
    }
    for index in 0..count {
        let element = page_offset + PAGE_HEADER_SIZE + index * BRANCH_ELEMENT_SIZE;
        let child_page = read_u64(state.bytes, element + 8)?;
        walk_page(child_page, depth + 1, state, bucket_path.clone())?;
    }
    Ok(())
}

fn display_key(bytes: &[u8]) -> String {
    if bytes
        .iter()
        .all(|byte| byte.is_ascii_graphic() || *byte == b' ')
    {
        String::from_utf8_lossy(bytes).into_owned()
    } else {
        format!("hex:{}", hex::encode(bytes))
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    let end = offset
        .checked_add(2)
        .ok_or(KubernetesParserError::Truncated {
            offset: offset as u64,
            context: "u16",
        })?;
    let value = bytes
        .get(offset..end)
        .ok_or(KubernetesParserError::Truncated {
            offset: offset as u64,
            context: "u16",
        })?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let end = offset
        .checked_add(4)
        .ok_or(KubernetesParserError::Truncated {
            offset: offset as u64,
            context: "u32",
        })?;
    let value = bytes
        .get(offset..end)
        .ok_or(KubernetesParserError::Truncated {
            offset: offset as u64,
            context: "u32",
        })?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64> {
    let end = offset
        .checked_add(8)
        .ok_or(KubernetesParserError::Truncated {
            offset: offset as u64,
            context: "u64",
        })?;
    let value = bytes
        .get(offset..end)
        .ok_or(KubernetesParserError::Truncated {
            offset: offset as u64,
            context: "u64",
        })?;
    Ok(u64::from_le_bytes([
        value[0], value[1], value[2], value[3], value[4], value[5], value[6], value[7],
    ]))
}
