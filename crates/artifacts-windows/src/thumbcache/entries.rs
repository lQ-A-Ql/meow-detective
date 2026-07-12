use byteorder::{LittleEndian, ReadBytesExt};
use std::io::{Cursor, Seek, SeekFrom};

const MAX_ENTRIES: u32 = 10_000;
const MAX_REPORTED_ENTRIES: usize = 100;
const ENTRY_HEADER_SIZE: u64 = 16;

#[derive(Default)]
pub(super) struct EntrySummary {
    pub(super) count: u32,
    pub(super) total_data_size: u64,
    pub(super) entries: Vec<serde_json::Value>,
}

pub(super) fn enumerate(data: &[u8], entry_offset: u32) -> EntrySummary {
    if entry_offset < 24 || entry_offset as usize >= data.len() {
        return EntrySummary::default();
    }
    let mut cursor = Cursor::new(data);
    if cursor.seek(SeekFrom::Start(entry_offset as u64)).is_err() {
        return EntrySummary::default();
    }
    let mut summary = EntrySummary::default();
    while summary.count < MAX_ENTRIES {
        let base = cursor.position() as usize;
        if base + ENTRY_HEADER_SIZE as usize > data.len() {
            break;
        }
        let Some(entry) = read_entry(&mut cursor) else {
            break;
        };
        summary.count += 1;
        summary.total_data_size += entry.data_size as u64;
        if summary.entries.len() < MAX_REPORTED_ENTRIES {
            summary.entries.push(entry.as_json());
        }
        if entry.size == ENTRY_HEADER_SIZE
            || cursor
                .seek(SeekFrom::Start(base as u64 + entry.size))
                .is_err()
        {
            break;
        }
    }
    summary
}

#[derive(Default)]
struct Entry {
    size: u64,
    hash_high: u32,
    hash_low: u32,
    data_size: u32,
}

impl Entry {
    fn as_json(&self) -> serde_json::Value {
        serde_json::json!({
            "hash": format!("{:08X}{:08X}", self.hash_high, self.hash_low),
            "data_size": self.data_size,
        })
    }
}

fn read_entry(cursor: &mut Cursor<&[u8]>) -> Option<Entry> {
    let size = cursor.read_u32::<LittleEndian>().ok()? as u64;
    if size < ENTRY_HEADER_SIZE {
        return None;
    }
    Some(Entry {
        size,
        hash_high: cursor.read_u32::<LittleEndian>().ok()?,
        hash_low: cursor.read_u32::<LittleEndian>().ok()?,
        data_size: cursor.read_u32::<LittleEndian>().ok()?,
    })
}
