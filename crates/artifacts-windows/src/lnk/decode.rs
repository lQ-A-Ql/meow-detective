use super::shell_item;
use byteorder::{LittleEndian, ReadBytesExt};
use chrono::{DateTime, Datelike, TimeZone, Utc};
use std::io::Read;

const HAS_LINK_TARGET_ID_LIST: u32 = 0x0000_0001;
const HAS_LINK_INFO: u32 = 0x0000_0002;

pub(super) struct DecodedLnk {
    pub(super) file_size: u32,
    pub(super) target_path: String,
    pub(super) timestamps: Vec<LnkTimestamp>,
}

pub(super) struct LnkTimestamp {
    pub(super) field: &'static str,
    pub(super) event_type: &'static str,
    pub(super) value: DateTime<Utc>,
}

pub(super) fn decode_lnk(reader: &mut impl Read) -> Result<DecodedLnk, String> {
    let header_size = reader.read_u32::<LittleEndian>().unwrap_or(0x4c);
    if header_size < 0x4c {
        return Err("LNK header too small".to_string());
    }
    let mut clsid = [0u8; 16];
    reader
        .read_exact(&mut clsid)
        .map_err(|error| error.to_string())?;
    let flags = reader.read_u32::<LittleEndian>().unwrap_or(0);
    let _file_attributes = reader.read_u32::<LittleEndian>().unwrap_or(0);
    let creation_time = reader.read_u64::<LittleEndian>().unwrap_or(0);
    let access_time = reader.read_u64::<LittleEndian>().unwrap_or(0);
    let write_time = reader.read_u64::<LittleEndian>().unwrap_or(0);
    let file_size = reader.read_u32::<LittleEndian>().unwrap_or(0);
    skip_header_tail(reader);

    let id_list_path = read_id_list_path(reader, flags);
    let link_info_path = read_link_info_path(reader, flags);
    let target_path = link_info_path
        .filter(|path| !path.is_empty())
        .unwrap_or(id_list_path);
    Ok(DecodedLnk {
        file_size,
        target_path,
        timestamps: collect_timestamps(creation_time, access_time, write_time),
    })
}

fn skip_header_tail(reader: &mut impl Read) {
    let _icon_index = reader.read_i32::<LittleEndian>().unwrap_or(0);
    let _show_command = reader.read_u32::<LittleEndian>().unwrap_or(0);
    let mut tail = [0u8; 12];
    let _ = reader.read_exact(&mut tail);
}

fn read_id_list_path(reader: &mut impl Read, flags: u32) -> String {
    if flags & HAS_LINK_TARGET_ID_LIST == 0 {
        return String::new();
    }
    let size = reader.read_u16::<LittleEndian>().unwrap_or(0) as usize;
    if size <= 2 {
        return String::new();
    }
    let mut data = vec![0u8; size - 2];
    let _ = reader.read_exact(&mut data);
    shell_item::parse_shell_items(&data)
}

fn read_link_info_path(reader: &mut impl Read, flags: u32) -> Option<String> {
    if flags & HAS_LINK_INFO == 0 {
        return None;
    }
    let size = reader.read_u32::<LittleEndian>().unwrap_or(0);
    if size < 28 {
        return None;
    }
    let _flags = reader.read_u32::<LittleEndian>().unwrap_or(0);
    let _volume_offset = reader.read_u32::<LittleEndian>().unwrap_or(0);
    let path_offset = reader.read_u32::<LittleEndian>().unwrap_or(0);
    if path_offset < 16 || path_offset as u64 >= size as u64 {
        return None;
    }
    let mut skipped = vec![0u8; path_offset.saturating_sub(16).min(256) as usize];
    let _ = reader.read_exact(&mut skipped);
    read_null_string(
        reader,
        (size as usize)
            .saturating_sub(path_offset as usize)
            .min(520),
    )
}

fn read_null_string(reader: &mut impl Read, max_bytes: usize) -> Option<String> {
    let mut buffer = vec![0u8; max_bytes];
    let read = reader.read(&mut buffer).ok()?;
    buffer.truncate(read);
    let end = buffer.iter().position(|byte| *byte == 0).unwrap_or(read);
    let bytes = buffer[..end].to_vec();
    (!bytes.is_empty())
        .then(|| String::from_utf8(bytes).ok())
        .flatten()
}

fn collect_timestamps(creation: u64, access: u64, write: u64) -> Vec<LnkTimestamp> {
    [
        ("creation_time", "LINK_CREATED", creation),
        ("access_time", "LINK_ACCESSED", access),
        ("write_time", "LINK_MODIFIED", write),
    ]
    .into_iter()
    .filter_map(|(field, event_type, value)| {
        filetime_to_datetime(value)
            .filter(|timestamp| timestamp.year() > 2000 && timestamp.year() < 2100)
            .map(|value| LnkTimestamp {
                field,
                event_type,
                value,
            })
    })
    .collect()
}

fn filetime_to_datetime(value: u64) -> Option<DateTime<Utc>> {
    if value == 0 || value >= 0x8000_0000_0000_0000 {
        return None;
    }
    let seconds = (value / 10_000_000) as i64 - 11_644_473_600;
    Utc.timestamp_opt(seconds, ((value % 10_000_000) * 100) as u32)
        .single()
}
