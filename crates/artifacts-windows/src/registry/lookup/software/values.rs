use super::super::{RegistryHiveReader, RegistryValue};

pub(super) fn read_optional_string_value(
    hive: &RegistryHiveReader<'_>,
    key_path: &[&str],
    value_name: &str,
) -> Option<String> {
    match hive.lookup_value(key_path, value_name) {
        Ok(Some(RegistryValue::String(value))) if !value.trim().is_empty() => Some(value),
        _ => None,
    }
}

pub(super) fn read_optional_dword_value(
    hive: &RegistryHiveReader<'_>,
    key_path: &[&str],
    value_name: &str,
) -> Option<u32> {
    match hive.lookup_value(key_path, value_name) {
        Ok(Some(RegistryValue::Dword(value))) => Some(value),
        _ => None,
    }
}

pub(super) fn read_optional_binary_value(
    hive: &RegistryHiveReader<'_>,
    key_path: &[&str],
    value_name: &str,
) -> Option<Vec<u8>> {
    match hive.lookup_value(key_path, value_name) {
        Ok(Some(RegistryValue::Binary(value))) if !value.is_empty() => Some(value),
        _ => None,
    }
}

pub(super) fn systemtime_bytes_to_rfc3339(data: &[u8]) -> Option<String> {
    if data.len() != 16 {
        return None;
    }
    let read_u16 = |offset: usize| u16::from_le_bytes([data[offset], data[offset + 1]]);
    let date = chrono::NaiveDate::from_ymd_opt(
        read_u16(0) as i32,
        read_u16(2) as u32,
        read_u16(6) as u32,
    )?;
    let time = chrono::NaiveTime::from_hms_milli_opt(
        read_u16(8) as u32,
        read_u16(10) as u32,
        read_u16(12) as u32,
        read_u16(14) as u32,
    )?;
    Some(
        chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
            chrono::NaiveDateTime::new(date, time),
            chrono::Utc,
        )
        .to_rfc3339(),
    )
}
