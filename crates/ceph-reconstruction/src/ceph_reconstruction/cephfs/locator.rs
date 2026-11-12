use std::ops::Range;

use super::CephFsInventoryError;

const MAX_NAMESPACE_BYTES: usize = 4096;
const MAX_OBJECT_NAME_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CephFsObjectLocator {
    filesystem_id: i64,
    pool_id: i64,
    namespace: Vec<u8>,
    object_name: Vec<u8>,
    fsmap_epoch: u32,
}

impl CephFsObjectLocator {
    pub fn new(
        filesystem_id: i64,
        pool_id: i64,
        namespace: Vec<u8>,
        object_name: Vec<u8>,
        fsmap_epoch: u32,
    ) -> Result<Self, CephFsInventoryError> {
        if filesystem_id < 0
            || pool_id < 0
            || fsmap_epoch == 0
            || namespace.len() > MAX_NAMESPACE_BYTES
            || object_name.is_empty()
            || object_name.len() > MAX_OBJECT_NAME_BYTES
        {
            return Err(CephFsInventoryError::InvalidLocator);
        }
        Ok(Self {
            filesystem_id,
            pool_id,
            namespace,
            object_name,
            fsmap_epoch,
        })
    }

    pub fn parse(value: &str) -> Result<Self, CephFsInventoryError> {
        let mut parts = value.split(':');
        let filesystem_id = parse_canonical_i64(parts.next())?;
        let pool_id = parse_canonical_i64(parts.next())?;
        let namespace = parse_hex_bytes(parts.next(), true)?;
        let object_name = parse_hex_bytes(parts.next(), false)?;
        let fsmap_epoch = parse_canonical_u32(parts.next())?;
        if parts.next().is_some() {
            return Err(CephFsInventoryError::InvalidLocator);
        }
        let locator = Self::new(filesystem_id, pool_id, namespace, object_name, fsmap_epoch)?;
        if locator.canonical() != value {
            return Err(CephFsInventoryError::InvalidLocator);
        }
        Ok(locator)
    }

    pub fn canonical(&self) -> String {
        format!(
            "{}:{}:h{}:h{}:{}",
            self.filesystem_id,
            self.pool_id,
            hex::encode(&self.namespace),
            hex::encode(&self.object_name),
            self.fsmap_epoch
        )
    }

    pub fn filesystem_id(&self) -> i64 {
        self.filesystem_id
    }

    pub fn pool_id(&self) -> i64 {
        self.pool_id
    }

    pub fn namespace(&self) -> &[u8] {
        &self.namespace
    }

    pub fn object_name(&self) -> &[u8] {
        &self.object_name
    }

    pub fn fsmap_epoch(&self) -> u32 {
        self.fsmap_epoch
    }

    pub fn checked_range(
        &self,
        offset: u64,
        length: usize,
        object_size: u64,
    ) -> Result<Range<u64>, CephFsInventoryError> {
        let length = u64::try_from(length).map_err(|_| CephFsInventoryError::RangeOverflow {
            locator: self.canonical(),
        })?;
        let end =
            offset
                .checked_add(length)
                .ok_or_else(|| CephFsInventoryError::RangeOverflow {
                    locator: self.canonical(),
                })?;
        if end > object_size {
            return Err(CephFsInventoryError::RangeOutOfBounds {
                locator: self.canonical(),
                object_size,
            });
        }
        Ok(offset..end)
    }
}

fn parse_canonical_i64(value: Option<&str>) -> Result<i64, CephFsInventoryError> {
    let value = value.ok_or(CephFsInventoryError::InvalidLocator)?;
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(CephFsInventoryError::InvalidLocator);
    }
    value
        .parse()
        .map_err(|_| CephFsInventoryError::InvalidLocator)
}

fn parse_canonical_u32(value: Option<&str>) -> Result<u32, CephFsInventoryError> {
    let value = value.ok_or(CephFsInventoryError::InvalidLocator)?;
    if value.is_empty()
        || value == "0"
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(CephFsInventoryError::InvalidLocator);
    }
    value
        .parse()
        .map_err(|_| CephFsInventoryError::InvalidLocator)
}

fn parse_hex_bytes(
    value: Option<&str>,
    allow_empty: bool,
) -> Result<Vec<u8>, CephFsInventoryError> {
    let value = value
        .and_then(|value| value.strip_prefix('h'))
        .ok_or(CephFsInventoryError::InvalidLocator)?;
    if (!allow_empty && value.is_empty())
        || !value.len().is_multiple_of(2)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(CephFsInventoryError::InvalidLocator);
    }
    hex::decode(value).map_err(|_| CephFsInventoryError::InvalidLocator)
}
