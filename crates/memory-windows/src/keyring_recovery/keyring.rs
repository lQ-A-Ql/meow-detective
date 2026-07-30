use volume_bitlocker::RecoveredVmk;
use zeroize::Zeroizing;

use crate::{MemoryWindowsError, Result, X64AddressSpace};

use super::profile::KeyringLayout;

const KEYRING_SIGNATURE: &[u8; 8] = b"-FVE-FS-";
const KEYRING_VERSION: u32 = 1;
const DATASET_ACTIVE_FLAG: u32 = 1;
const VMK_DATUM_SIZE: usize = 44;
const VMK_DATUM_ENTRY_TYPE: u16 = 5;
const RAW_KEY_VALUE_TYPE: u16 = 1;
const VMK_DATUM_VERSION: u16 = 1;
const VMK_ALGORITHM: u16 = 0x2003;
const VMK_OFFSET: usize = 12;
const VMK_LENGTH: usize = 32;

pub(crate) struct ParsedKeyringVmk {
    pub vmk: RecoveredVmk,
    pub datasets_examined: usize,
}

pub(crate) fn read_matching_vmk(
    address_space: &mut X64AddressSpace,
    keyring_address: u64,
    volume_guid: [u8; 16],
    layout: KeyringLayout,
) -> Result<ParsedKeyringVmk> {
    let mut bytes = Zeroizing::new(vec![0u8; layout.capacity as usize]);
    address_space.read_virtual_exact(keyring_address, &mut bytes)?;
    parse_matching_vmk(&bytes, volume_guid, layout)
}

pub(crate) fn parse_matching_vmk(
    bytes: &[u8],
    volume_guid: [u8; 16],
    layout: KeyringLayout,
) -> Result<ParsedKeyringVmk> {
    let (first_dataset, end) = validate_header(bytes, layout)?;
    let mut offset = first_dataset;
    let mut datasets_examined = 0usize;
    let mut recovered = None;
    while offset < end {
        datasets_examined += 1;
        let dataset = parse_dataset(bytes, offset, end, layout)?;
        if dataset.active && dataset.volume_guid == volume_guid {
            let vmk = parse_dataset_vmk(dataset.bytes)?;
            if recovered.replace(vmk).is_some() {
                return Err(MemoryWindowsError::AmbiguousBitLockerVolumeDataset);
            }
        }
        offset = offset
            .checked_add(dataset.total_length)
            .ok_or(MemoryWindowsError::MalformedBitLockerKeyring)?;
    }
    if offset != end {
        return Err(MemoryWindowsError::MalformedBitLockerKeyring);
    }
    Ok(ParsedKeyringVmk {
        vmk: recovered.ok_or(MemoryWindowsError::BitLockerVolumeDatasetNotFound)?,
        datasets_examined,
    })
}

struct Dataset<'a> {
    bytes: &'a [u8],
    total_length: usize,
    active: bool,
    volume_guid: [u8; 16],
}

fn validate_header(bytes: &[u8], layout: KeyringLayout) -> Result<(usize, usize)> {
    if bytes.len() != layout.capacity as usize
        || bytes.get(..8) != Some(KEYRING_SIGNATURE)
        || read_u32(bytes, 8)? != layout.capacity
        || read_u32(bytes, 12)? != KEYRING_VERSION
    {
        return Err(MemoryWindowsError::MalformedBitLockerKeyring);
    }
    let first = read_u32(bytes, 16)? as usize;
    let end = read_u32(bytes, 20)? as usize;
    if first != layout.header_size as usize
        || end < first
        || end > bytes.len()
        || !first.is_multiple_of(16)
        || !end.is_multiple_of(16)
    {
        return Err(MemoryWindowsError::MalformedBitLockerKeyring);
    }
    Ok((first, end))
}

fn parse_dataset(
    bytes: &[u8],
    offset: usize,
    keyring_end: usize,
    layout: KeyringLayout,
) -> Result<Dataset<'_>> {
    let header_end = offset
        .checked_add(layout.dataset_minimum_size as usize)
        .ok_or(MemoryWindowsError::MalformedBitLockerKeyring)?;
    if header_end > keyring_end {
        return Err(MemoryWindowsError::MalformedBitLockerKeyring);
    }
    let total_length = read_u32(bytes, offset)? as usize;
    let flags = read_u32(bytes, offset + 4)?;
    let datum_start = read_u32(bytes, offset + 8)? as usize;
    let datum_end = read_u32(bytes, offset + 12)? as usize;
    let dataset_end = offset
        .checked_add(total_length)
        .ok_or(MemoryWindowsError::MalformedBitLockerKeyring)?;
    if total_length < layout.dataset_minimum_size as usize
        || !total_length.is_multiple_of(16)
        || dataset_end > keyring_end
        || datum_start < layout.dataset_minimum_size as usize
        || datum_start > datum_end
        || datum_end > total_length
    {
        return Err(MemoryWindowsError::MalformedBitLockerKeyring);
    }
    let guid_start = offset + usize::from(layout.dataset_volume_guid_offset);
    let volume_guid = bytes
        .get(guid_start..guid_start + 16)
        .and_then(|value| value.try_into().ok())
        .ok_or(MemoryWindowsError::MalformedBitLockerKeyring)?;
    let datum_bytes = bytes
        .get(offset + datum_start..offset + datum_end)
        .ok_or(MemoryWindowsError::MalformedBitLockerKeyring)?;
    Ok(Dataset {
        bytes: datum_bytes,
        total_length,
        active: flags & 0x7FFF_FFFF == DATASET_ACTIVE_FLAG,
        volume_guid,
    })
}

fn parse_dataset_vmk(bytes: &[u8]) -> Result<RecoveredVmk> {
    let mut offset = 0usize;
    let mut recovered = None;
    while offset < bytes.len() {
        let header = bytes
            .get(offset..offset + 8)
            .ok_or(MemoryWindowsError::MalformedBitLockerKeyring)?;
        let size = u16::from_le_bytes([header[0], header[1]]) as usize;
        let end = offset
            .checked_add(size)
            .ok_or(MemoryWindowsError::MalformedBitLockerKeyring)?;
        if size < 8 || end > bytes.len() {
            return Err(MemoryWindowsError::MalformedBitLockerKeyring);
        }
        if let Some(vmk) = parse_exact_vmk_datum(&bytes[offset..end]) {
            if recovered.replace(RecoveredVmk::new(vmk)).is_some() {
                return Err(MemoryWindowsError::AmbiguousBitLockerVmkDatum);
            }
        }
        offset = end;
    }
    recovered.ok_or(MemoryWindowsError::BitLockerVmkDatumNotFound)
}

pub(crate) fn parse_exact_vmk_datum(bytes: &[u8]) -> Option<[u8; VMK_LENGTH]> {
    let matches = bytes.len() == VMK_DATUM_SIZE
        && u16::from_le_bytes([bytes[2], bytes[3]]) == VMK_DATUM_ENTRY_TYPE
        && u16::from_le_bytes([bytes[4], bytes[5]]) == RAW_KEY_VALUE_TYPE
        && u16::from_le_bytes([bytes[6], bytes[7]]) == VMK_DATUM_VERSION
        && u16::from_le_bytes([bytes[8], bytes[9]]) == VMK_ALGORITHM;
    matches.then(|| {
        let mut vmk = [0u8; VMK_LENGTH];
        vmk.copy_from_slice(&bytes[VMK_OFFSET..VMK_OFFSET + VMK_LENGTH]);
        vmk
    })
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    bytes
        .get(offset..offset + 4)
        .and_then(|value| value.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or(MemoryWindowsError::MalformedBitLockerKeyring)
}
