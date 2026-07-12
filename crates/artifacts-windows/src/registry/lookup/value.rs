use super::types::{RegistryValue, REG_DWORD, REG_EXPAND_SZ, REG_MULTI_SZ, REG_QWORD, REG_SZ};
use super::utf16;

pub(crate) fn parse_value_data(data_type: u32, data: &[u8]) -> Result<RegistryValue, String> {
    match data_type {
        REG_SZ | REG_EXPAND_SZ => Ok(RegistryValue::String(utf16::decode_utf16_until_nul(data)?)),
        REG_DWORD => Ok(RegistryValue::Dword(
            utf16::read_le_array::<4>(data)
                .map(u32::from_le_bytes)
                .ok_or_else(|| "REG_DWORD value shorter than 4 bytes".to_string())?,
        )),
        REG_QWORD => Ok(RegistryValue::Qword(
            utf16::read_le_array::<8>(data)
                .map(u64::from_le_bytes)
                .ok_or_else(|| "REG_QWORD value shorter than 8 bytes".to_string())?,
        )),
        REG_MULTI_SZ => Ok(RegistryValue::MultiString(
            utf16::decode_utf16_full(data)?
                .split('\0')
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect(),
        )),
        _ => Ok(RegistryValue::Binary(data.to_vec())),
    }
}
