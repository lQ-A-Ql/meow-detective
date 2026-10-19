//! Windows RawSMBIOSData framing and bounded SMBIOS structure/string traversal.
//! Layout: Microsoft GetSystemFirmwareTable; DMTF DSP0134 3.8.0 sections 6/7.
use super::{
    smbios_records,
    types::{MAX_DEVICES, MAX_TEXT_BYTES},
    RegistryHardwareRecord,
};
use crate::registry::RegistryError;

const MAX_TABLE_BYTES: usize = 8 * 1024 * 1024;

pub(super) struct Structure<'a> {
    pub formatted: &'a [u8],
    pub strings: &'a [u8],
    pub offset: u32,
}

pub(super) fn parse(
    bytes: &[u8],
    key_path: &str,
) -> Result<Vec<RegistryHardwareRecord>, RegistryError> {
    if bytes.len() < 8 || !(2..=3).contains(&bytes[1]) {
        return Err(RegistryError::other("invalid RawSMBIOSData header"));
    }
    let size = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    if size > MAX_TABLE_BYTES || size > bytes.len() - 8 {
        return Err(RegistryError::other(
            "SMBIOS table length exceeds its buffer or budget",
        ));
    }
    let table = &bytes[8..8 + size];
    let mut offset = 0;
    let mut devices = Vec::new();
    let mut structures = 0;
    while offset < table.len() {
        if table.len() - offset < 4 || structures >= 65536 {
            return Err(RegistryError::other(
                "invalid SMBIOS structure boundary or count",
            ));
        }
        let length = table[offset + 1] as usize;
        if length < 4 || length > table.len() - offset {
            return Err(RegistryError::other("invalid SMBIOS formatted length"));
        }
        let tail = &table[offset + length..];
        let end = tail
            .windows(2)
            .position(|pair| pair == [0, 0])
            .ok_or_else(|| RegistryError::other("unterminated SMBIOS string set"))?;
        let entry = Structure {
            formatted: &table[offset..offset + length],
            strings: &tail[..end],
            offset: (8 + offset) as u32,
        };
        if entry.formatted[0] == 127 {
            break;
        }
        if let Some(record) = smbios_records::decode(&entry, key_path) {
            if devices.len() >= MAX_DEVICES {
                return Err(RegistryError::other("SMBIOS device budget exceeded"));
            }
            devices.push(record);
        }
        offset += length + end + 2;
        structures += 1;
    }
    Ok(devices)
}

impl Structure<'_> {
    pub fn string(&self, offset: usize) -> Option<String> {
        let index = usize::from(*self.formatted.get(offset)?);
        if index == 0 {
            return None;
        }
        let bytes = self.strings.split(|byte| *byte == 0).nth(index - 1)?;
        if bytes.len() > MAX_TEXT_BYTES {
            return None;
        }
        let text = String::from_utf8_lossy(bytes).trim().to_string();
        (!text.is_empty()).then_some(text)
    }

    pub fn word(&self, offset: usize) -> Option<u16> {
        let raw = self.formatted.get(offset..offset + 2)?;
        Some(u16::from_le_bytes([raw[0], raw[1]]))
    }

    pub fn dword(&self, offset: usize) -> Option<u32> {
        let raw = self.formatted.get(offset..offset + 4)?;
        Some(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
    }

    pub fn count(&self, short: usize, extended: usize) -> Option<u16> {
        match *self.formatted.get(short)? {
            0 => None,
            255 => match self.word(extended) {
                Some(0 | u16::MAX) => None,
                Some(value) => Some(value),
                None => Some(255),
            },
            value => Some(u16::from(value)),
        }
    }

    pub fn speed(&self, short: usize, extended: usize) -> Option<u32> {
        let value = match self.word(short)? {
            u16::MAX => self.dword(extended).filter(|v| *v < 0x8000_0000)?,
            value => u32::from(value),
        };
        (value != 0).then_some(value)
    }
}
