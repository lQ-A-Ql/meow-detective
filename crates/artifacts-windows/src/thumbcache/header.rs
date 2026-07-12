use byteorder::{LittleEndian, ReadBytesExt};
use std::io::{Cursor, Seek, SeekFrom};

const THUMBCACHE_MAGIC: &[u8; 4] = b"CMMM";
const THUMBCACHE_MAGIC_2: &[u8; 4] = b"ISM1";

pub(super) struct ThumbcacheHeader {
    pub(super) header_size: u32,
    pub(super) version: u32,
    pub(super) cache_type: u32,
    pub(super) entry_offset: u32,
}

impl ThumbcacheHeader {
    pub(super) fn parse(data: &[u8]) -> Result<Self, String> {
        if data.len() < 24 {
            return Err("File too small for thumbcache header".to_string());
        }
        if &data[0..4] != THUMBCACHE_MAGIC && &data[0..4] != THUMBCACHE_MAGIC_2 {
            return Err("Not a valid thumbcache file".to_string());
        }
        let mut cursor = Cursor::new(data);
        cursor
            .seek(SeekFrom::Start(4))
            .map_err(|error| error.to_string())?;
        let header_size = cursor.read_u32::<LittleEndian>().unwrap_or(0);
        let version = cursor.read_u32::<LittleEndian>().unwrap_or(0);
        let cache_type = cursor.read_u32::<LittleEndian>().unwrap_or(0);
        cursor
            .seek(SeekFrom::Start(16))
            .map_err(|error| error.to_string())?;
        let entry_offset = cursor.read_u32::<LittleEndian>().unwrap_or(0);
        Ok(Self {
            header_size,
            version,
            cache_type,
            entry_offset,
        })
    }

    pub(super) fn cache_type_description(&self) -> &'static str {
        match self.cache_type {
            0x01 => "32x32",
            0x02 => "96x96",
            0x03 => "256x256",
            0x04 => "1024x1024",
            0x05 => "16x16",
            _ => "Unknown",
        }
    }
}
