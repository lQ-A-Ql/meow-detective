use super::super::{RegistryHiveReader, RegistryValue};

pub(super) fn machine_sid_from_v_data(value_data: &[u8], warnings: &mut Vec<String>) -> String {
    const SID_OFFSET: usize = 408;
    if value_data.len() < SID_OFFSET + 16 {
        warnings.push(format!(
            "SAM Domain Account V value is too short to contain machine SID ({} bytes)",
            value_data.len()
        ));
        return String::new();
    }
    let read_u32 = |offset: usize| {
        u32::from_le_bytes(
            value_data[offset..offset + 4]
                .try_into()
                .expect("slice length is 4"),
        )
    };
    format!(
        "S-1-5-{}-{}-{}-{}",
        read_u32(SID_OFFSET),
        read_u32(SID_OFFSET + 4),
        read_u32(SID_OFFSET + 8),
        read_u32(SID_OFFSET + 12)
    )
}

pub(super) fn extract_machine_sid(
    hive: &RegistryHiveReader<'_>,
    warnings: &mut Vec<String>,
) -> String {
    match hive.lookup_value(&["SAM", "Domains", "Account"], "V") {
        Ok(Some(RegistryValue::Binary(data))) => machine_sid_from_v_data(&data, warnings),
        Ok(Some(other)) => {
            warnings.push(format!(
                "SAM Domain Account V value has unexpected type: {:?}",
                other
            ));
            String::new()
        }
        Ok(None) => {
            warnings.push("SAM Domain Account V value not found".to_string());
            String::new()
        }
        Err(error) => {
            warnings.push(format!("SAM Domain Account V parse error: {error}"));
            String::new()
        }
    }
}

pub(super) fn sid_bytes_to_string(data: &[u8]) -> Option<String> {
    if data.len() < 8 {
        return None;
    }
    let revision = data[0];
    let sub_authority_count = data[1] as usize;
    if sub_authority_count == 0 || sub_authority_count > 15 {
        return None;
    }
    let sid_len = 8usize.checked_add(sub_authority_count.checked_mul(4)?)?;
    if data.len() < sid_len {
        return None;
    }
    let authority =
        u64::from_be_bytes([0, 0, data[2], data[3], data[4], data[5], data[6], data[7]]);
    let sub_authorities = (0..sub_authority_count)
        .map(|index| {
            let offset = 8 + index * 4;
            u32::from_le_bytes(
                data[offset..offset + 4]
                    .try_into()
                    .expect("four-byte sub-authority"),
            )
        })
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join("-");
    Some(format!("S-{revision}-{authority}-{sub_authorities}"))
}

pub(super) fn parse_sid_at_end(data: &[u8]) -> Option<(String, u32, usize)> {
    for length in [28usize, 12usize] {
        if data.len() < length {
            continue;
        }
        let bytes = &data[data.len() - length..];
        if let Some(sid) = sid_bytes_to_string(bytes) {
            let rid = u32::from_le_bytes(bytes[length - 4..length].try_into().ok()?);
            return Some((sid, rid, length));
        }
    }
    None
}
