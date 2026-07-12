use chrono::{DateTime, TimeZone, Utc};

pub(crate) fn rot13_decode(encoded: &str) -> String {
    encoded
        .chars()
        .map(|c| match c {
            'a'..='m' | 'A'..='M' => ((c as u8) + 13) as char,
            'n'..='z' | 'N'..='Z' => ((c as u8) - 13) as char,
            _ => c,
        })
        .collect()
}

pub(crate) fn windows_filetime_to_rfc3339(filetime: u64) -> Option<String> {
    filetime_to_utc(filetime).map(|dt| dt.to_rfc3339())
}

pub(crate) fn extract_utf16le_from_binary(data: &[u8]) -> Option<String> {
    if data.len() < 2 {
        return None;
    }
    let payload = if data.len() >= 4 {
        let header = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if header > 0 && header.saturating_sub(4) <= data.len().saturating_sub(4) {
            &data[4..]
        } else {
            data
        }
    } else {
        data
    };
    let units: Vec<u16> = payload
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .take_while(|unit| *unit != 0)
        .collect();
    (!units.is_empty()).then(|| String::from_utf16_lossy(&units))
}

pub(crate) fn filetime_to_utc(filetime: u64) -> Option<DateTime<Utc>> {
    if filetime == 0 {
        return None;
    }
    let unix_seconds = (filetime / 10_000_000).saturating_sub(11_644_473_600);
    let nanos = ((filetime % 10_000_000) * 100) as u32;
    Utc.timestamp_opt(unix_seconds as i64, nanos).single()
}
