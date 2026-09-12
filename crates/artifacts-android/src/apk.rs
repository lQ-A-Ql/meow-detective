use std::io::{Read, Seek};

use thiserror::Error;
use zip::{result::ZipError, ZipArchive};

use super::apk_strings::StringPool;
pub(crate) const RES_STRING_POOL_TYPE: u16 = 0x0001;
const RES_TABLE_TYPE: u16 = 0x0002;
const RES_XML_TYPE: u16 = 0x0003;
const RES_TABLE_PACKAGE_TYPE: u16 = 0x0200;
const RES_TABLE_TYPE_TYPE: u16 = 0x0201;
const RES_XML_RESOURCE_MAP_TYPE: u16 = 0x0180;
const RES_XML_START_ELEMENT_TYPE: u16 = 0x0102;
const TYPE_REFERENCE: u8 = 0x01;
const TYPE_STRING: u8 = 0x03;
const NO_ENTRY: u32 = u32::MAX;
const FLAG_COMPLEX: u16 = 0x0001;
const ANDROID_ATTR_LABEL: u32 = 0x0101_0001;
const ANDROID_ATTR_ICON: u32 = 0x0101_0002;
const MAX_MANIFEST_BYTES: usize = 8 * 1024 * 1024;
const MAX_RESOURCES_BYTES: usize = 16 * 1024 * 1024;
const MAX_ICON_BYTES: usize = 128 * 1024;
const MAX_APK_ENTRIES: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidApkPresentation {
    pub app_name: Option<String>,
    pub icon: Option<AndroidApkIcon>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidApkIcon {
    pub mime_type: &'static str,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum ApkInspectError {
    #[error("APK archive is invalid: {0}")]
    Archive(String),
    #[error("APK resource data is invalid: {0}")]
    Resources(String),
}

#[derive(Debug, Clone)]
struct ManifestValues {
    label_string: Option<String>,
    label_resource: Option<u32>,
    icon_resource: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
struct ResourceValue {
    data_type: u8,
    data: u32,
}

/// Reads a bounded, presentation-only subset of one APK. The caller owns the
/// seekable evidence reader, so the APK is never extracted or materialized.
pub fn inspect_apk<R: Read + Seek>(reader: R) -> Result<AndroidApkPresentation, ApkInspectError> {
    let mut archive = ZipArchive::new(reader).map_err(archive_error)?;
    if archive.len() > MAX_APK_ENTRIES {
        return Err(ApkInspectError::Archive(format!(
            "APK has more than {MAX_APK_ENTRIES} archive entries"
        )));
    }
    let manifest = read_zip_entry(&mut archive, "AndroidManifest.xml", MAX_MANIFEST_BYTES)?;
    let Some(manifest) = manifest else {
        return Ok(empty_presentation());
    };
    let manifest_values = parse_binary_manifest(&manifest)?;
    let Some(resources) = read_zip_entry(&mut archive, "resources.arsc", MAX_RESOURCES_BYTES)?
    else {
        return Ok(presentation_without_resources(manifest_values));
    };
    let app_name = resolve_manifest_label(&resources, &manifest_values)?;
    let icon_path = resolve_icon_path(&resources, &manifest_values)?;
    let icon = match icon_path {
        Some(path) => read_icon_entry(&mut archive, &path)?,
        None => None,
    };
    Ok(AndroidApkPresentation { app_name, icon })
}

fn empty_presentation() -> AndroidApkPresentation {
    AndroidApkPresentation {
        app_name: None,
        icon: None,
    }
}

fn presentation_without_resources(values: ManifestValues) -> AndroidApkPresentation {
    AndroidApkPresentation {
        app_name: values.label_string,
        icon: None,
    }
}

fn archive_error(error: zip::result::ZipError) -> ApkInspectError {
    ApkInspectError::Archive(error.to_string())
}

fn read_zip_entry<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
    limit: usize,
) -> Result<Option<Vec<u8>>, ApkInspectError> {
    let mut entry = match archive.by_name(name) {
        Ok(entry) => entry,
        Err(ZipError::FileNotFound) => return Ok(None),
        Err(error) => return Err(archive_error(error)),
    };
    if entry.size() > limit as u64 {
        return Err(ApkInspectError::Resources(format!(
            "{name} exceeds the {limit} byte limit"
        )));
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry
        .read_to_end(&mut bytes)
        .map_err(|error| ApkInspectError::Archive(error.to_string()))?;
    Ok(Some(bytes))
}

fn read_icon_entry<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
) -> Result<Option<AndroidApkIcon>, ApkInspectError> {
    let Some(bytes) = read_zip_entry(archive, path, MAX_ICON_BYTES)? else {
        return Ok(None);
    };
    let mime_type = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    };
    Ok(mime_type.map(|mime_type| AndroidApkIcon { mime_type, bytes }))
}

fn parse_binary_manifest(bytes: &[u8]) -> Result<ManifestValues, ApkInspectError> {
    let root = chunk_at(bytes, 0)?;
    if root.chunk_type != RES_XML_TYPE || root.end != bytes.len() {
        return Err(ApkInspectError::Resources(
            "invalid binary Android manifest".to_string(),
        ));
    }
    let chunks = chunks_in(bytes, root.offset + root.header_size, root.end)?;
    let string_pool_chunk = chunks
        .iter()
        .find(|chunk| chunk.chunk_type == RES_STRING_POOL_TYPE)
        .ok_or_else(|| ApkInspectError::Resources("manifest string pool is missing".to_string()))?;
    let string_pool = StringPool::parse(bytes, string_pool_chunk.offset)?;
    let resource_map = chunks
        .iter()
        .find(|chunk| chunk.chunk_type == RES_XML_RESOURCE_MAP_TYPE)
        .map(|chunk| parse_resource_map(bytes, *chunk))
        .transpose()?
        .unwrap_or_default();

    for chunk in chunks {
        if chunk.chunk_type != RES_XML_START_ELEMENT_TYPE {
            continue;
        }
        let values = parse_application_attributes(bytes, chunk, &string_pool, &resource_map)?;
        if values.label_string.is_some()
            || values.label_resource.is_some()
            || values.icon_resource.is_some()
        {
            return Ok(values);
        }
    }
    Ok(ManifestValues {
        label_string: None,
        label_resource: None,
        icon_resource: None,
    })
}

fn parse_resource_map(bytes: &[u8], chunk: Chunk) -> Result<Vec<u32>, ApkInspectError> {
    let mut values = Vec::new();
    let mut offset = chunk.offset + chunk.header_size;
    while offset + 4 <= chunk.end {
        values.push(read_u32(bytes, offset)?);
        offset += 4;
    }
    Ok(values)
}

fn parse_application_attributes(
    bytes: &[u8],
    chunk: Chunk,
    strings: &StringPool<'_>,
    resource_map: &[u32],
) -> Result<ManifestValues, ApkInspectError> {
    let extension = chunk.offset + chunk.header_size;
    if extension + 20 > chunk.end {
        return Err(ApkInspectError::Resources(
            "manifest start-element extension is truncated".to_string(),
        ));
    }
    let element_name = read_u32(bytes, extension + 4)?;
    if strings.get(element_name)? != "application" {
        return Ok(empty_manifest_values());
    }
    let attribute_start = read_u16(bytes, extension + 8)? as usize;
    let attribute_size = read_u16(bytes, extension + 10)? as usize;
    let attribute_count = read_u16(bytes, extension + 12)? as usize;
    if attribute_size < 20 {
        return Err(ApkInspectError::Resources(
            "manifest attribute size is invalid".to_string(),
        ));
    }
    let first = extension + attribute_start;
    let last = first
        .checked_add(attribute_count.saturating_mul(attribute_size))
        .ok_or_else(|| ApkInspectError::Resources("manifest attribute overflow".to_string()))?;
    if first > chunk.end || last > chunk.end {
        return Err(ApkInspectError::Resources(
            "manifest attributes are truncated".to_string(),
        ));
    }
    let mut values = empty_manifest_values();
    for index in 0..attribute_count {
        let offset = first + index * attribute_size;
        let name_index = read_u32(bytes, offset + 4)?;
        let raw_value = read_u32(bytes, offset + 8)?;
        let data_type = read_u8(bytes, offset + 15)?;
        let data = read_u32(bytes, offset + 16)?;
        let resource_id = resource_map.get(name_index as usize).copied();
        let name = strings.get(name_index)?;
        let is_label = resource_id == Some(ANDROID_ATTR_LABEL) || name == "label";
        let is_icon = resource_id == Some(ANDROID_ATTR_ICON) || name == "icon";
        if is_label {
            assign_label(&mut values, strings, raw_value, data_type, data)?;
        }
        if is_icon && data_type == TYPE_REFERENCE {
            values.icon_resource = Some(data);
        }
    }
    Ok(values)
}

fn empty_manifest_values() -> ManifestValues {
    ManifestValues {
        label_string: None,
        label_resource: None,
        icon_resource: None,
    }
}

fn assign_label(
    values: &mut ManifestValues,
    strings: &StringPool<'_>,
    raw_value: u32,
    data_type: u8,
    data: u32,
) -> Result<(), ApkInspectError> {
    if raw_value != NO_ENTRY {
        values.label_string = Some(strings.get(raw_value)?);
    } else if data_type == TYPE_STRING {
        values.label_string = Some(strings.get(data)?);
    } else if data_type == TYPE_REFERENCE {
        values.label_resource = Some(data);
    }
    Ok(())
}

fn resolve_manifest_label(
    resources: &[u8],
    values: &ManifestValues,
) -> Result<Option<String>, ApkInspectError> {
    if let Some(resource_id) = values.label_resource {
        let table = ResourceTable::parse(resources)?;
        let Some(value) = table.value(resource_id)? else {
            return Ok(values.label_string.clone());
        };
        return (value.data_type == TYPE_STRING)
            .then(|| table.global_strings.get(value.data))
            .transpose()
            .map(|value| value.or_else(|| values.label_string.clone()));
    }
    Ok(values.label_string.clone())
}

fn resolve_icon_path(
    resources: &[u8],
    values: &ManifestValues,
) -> Result<Option<String>, ApkInspectError> {
    let Some(resource_id) = values.icon_resource else {
        return Ok(None);
    };
    let table = ResourceTable::parse(resources)?;
    let Some(value) = table.value(resource_id)? else {
        return Ok(None);
    };
    if value.data_type != TYPE_STRING {
        return Ok(None);
    }
    table.global_strings.get(value.data).map(Some)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Chunk {
    pub(crate) offset: usize,
    pub(crate) end: usize,
    pub(crate) header_size: usize,
    pub(crate) chunk_type: u16,
}

fn chunks_in(bytes: &[u8], start: usize, end: usize) -> Result<Vec<Chunk>, ApkInspectError> {
    let mut chunks = Vec::new();
    let mut offset = start;
    while offset < end {
        let chunk_type = read_u16(bytes, offset)?;
        let header_size = read_u16(bytes, offset + 2)? as usize;
        let size = read_u32(bytes, offset + 4)? as usize;
        let chunk_end = offset
            .checked_add(size)
            .ok_or_else(|| ApkInspectError::Resources("resource chunk overflow".to_string()))?;
        if size < 8 || header_size < 8 || header_size > size || chunk_end > end {
            return Err(ApkInspectError::Resources(
                "resource chunk has an invalid size".to_string(),
            ));
        }
        chunks.push(Chunk {
            offset,
            end: chunk_end,
            header_size,
            chunk_type,
        });
        offset = chunk_end;
    }
    Ok(chunks)
}

struct ResourceTable<'a> {
    bytes: &'a [u8],
    global_strings: StringPool<'a>,
    packages: Vec<Chunk>,
}

impl<'a> ResourceTable<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, ApkInspectError> {
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

    fn value(&self, resource_id: u32) -> Result<Option<ResourceValue>, ApkInspectError> {
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

pub(crate) fn chunk_at(bytes: &[u8], offset: usize) -> Result<Chunk, ApkInspectError> {
    chunks_in(bytes, offset, bytes.len())?
        .into_iter()
        .next()
        .ok_or_else(|| ApkInspectError::Resources("resource chunk is missing".to_string()))
}

pub(crate) fn read_u8(bytes: &[u8], offset: usize) -> Result<u8, ApkInspectError> {
    bytes
        .get(offset)
        .copied()
        .ok_or_else(|| ApkInspectError::Resources("resource data is truncated".to_string()))
}

pub(crate) fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, ApkInspectError> {
    let bytes = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| ApkInspectError::Resources("resource data is truncated".to_string()))?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

pub(crate) fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, ApkInspectError> {
    let bytes = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| ApkInspectError::Resources("resource data is truncated".to_string()))?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}
