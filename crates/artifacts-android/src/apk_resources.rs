use super::apk::{
    chunk_at, chunks_in, read_u16, read_u32, read_u8, ApkInspectError, Chunk, RES_STRING_POOL_TYPE,
};
use super::apk_strings::StringPool;

const RES_TABLE_TYPE: u16 = 0x0002;
const RES_TABLE_PACKAGE_TYPE: u16 = 0x0200;
const RES_TABLE_TYPE_TYPE: u16 = 0x0201;
const NO_ENTRY: u32 = u32::MAX;
const FLAG_COMPLEX: u16 = 0x0001;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ResourceValue {
    pub(crate) data_type: u8,
    pub(crate) data: u32,
}

pub(crate) struct ResourceTable<'a> {
    bytes: &'a [u8],
    pub(crate) global_strings: StringPool<'a>,
    packages: Vec<Chunk>,
}

impl<'a> ResourceTable<'a> {
    pub(crate) fn parse(bytes: &'a [u8]) -> Result<Self, ApkInspectError> {
        let root = chunk_at(bytes, 0)?;
        if root.chunk_type != RES_TABLE_TYPE || root.end != bytes.len() {
            return Err(ApkInspectError::Resources(
                "invalid resources table".to_string(),
            ));
        }
        let chunks = chunks_in(bytes, root.header_size, root.end)?;
        let global_chunk = chunks
            .iter()
            .find(|chunk| chunk.chunk_type == RES_STRING_POOL_TYPE)
            .ok_or_else(|| {
                ApkInspectError::Resources("global string pool is missing".to_string())
            })?;
        Ok(Self {
            bytes,
            global_strings: StringPool::parse(bytes, global_chunk.offset)?,
            packages: chunks
                .into_iter()
                .filter(|chunk| chunk.chunk_type == RES_TABLE_PACKAGE_TYPE)
                .collect(),
        })
    }

    pub(crate) fn value(&self, resource_id: u32) -> Result<Option<ResourceValue>, ApkInspectError> {
        let package_id = (resource_id >> 24) as u8;
        let type_id = ((resource_id >> 16) & 0xff) as u8;
        let entry_index = (resource_id & 0xffff) as usize;
        for package in &self.packages {
            if read_u32(self.bytes, package.offset + 8)? as u8 != package_id {
                continue;
            }
            if let Some(value) = self.package_value(*package, type_id, entry_index)? {
                return Ok(Some(value));
            }
        }
        Ok(None)
    }

    fn package_value(
        &self,
        package: Chunk,
        type_id: u8,
        entry_index: usize,
    ) -> Result<Option<ResourceValue>, ApkInspectError> {
        let chunks = chunks_in(
            self.bytes,
            package.offset + package.header_size,
            package.end,
        )?;
        for chunk in chunks {
            if chunk.chunk_type != RES_TABLE_TYPE_TYPE || chunk.header_size < 20 {
                continue;
            }
            if read_u8(self.bytes, chunk.offset + 8)? != type_id
                || !default_config(self.bytes, chunk)?
            {
                continue;
            }
            let entry_count = read_u32(self.bytes, chunk.offset + 12)? as usize;
            if entry_index >= entry_count {
                continue;
            }
            let entries_start = read_u32(self.bytes, chunk.offset + 16)? as usize;
            let index_offset = chunk.offset + chunk.header_size + entry_index * 4;
            let entry_offset = read_u32(self.bytes, index_offset)?;
            if entry_offset == NO_ENTRY {
                continue;
            }
            let entry = chunk.offset + entries_start + entry_offset as usize;
            if entry + 8 > chunk.end {
                return Err(ApkInspectError::Resources(
                    "resource entry is truncated".to_string(),
                ));
            }
            let entry_size = read_u16(self.bytes, entry)? as usize;
            let flags = read_u16(self.bytes, entry + 2)?;
            if entry_size < 8 || flags & FLAG_COMPLEX != 0 || entry + entry_size + 8 > chunk.end {
                continue;
            }
            let value = entry + entry_size;
            return Ok(Some(ResourceValue {
                data_type: read_u8(self.bytes, value + 3)?,
                data: read_u32(self.bytes, value + 4)?,
            }));
        }
        Ok(None)
    }
}

fn default_config(bytes: &[u8], chunk: Chunk) -> Result<bool, ApkInspectError> {
    let config_start = chunk.offset + 20;
    let config_size = read_u32(bytes, config_start)? as usize;
    let config_end = config_start
        .checked_add(config_size)
        .ok_or_else(|| ApkInspectError::Resources("resource config overflow".to_string()))?;
    if config_size < 4 || config_end > chunk.offset + chunk.header_size {
        return Err(ApkInspectError::Resources(
            "resource config is invalid".to_string(),
        ));
    }
    Ok(bytes[config_start + 4..config_end]
        .iter()
        .all(|byte| *byte == 0))
}
