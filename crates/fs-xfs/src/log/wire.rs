#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XfsLogFormat {
    Unknown,
    LinuxLittleEndian,
    LinuxBigEndian,
    IrixBigEndian,
    Other(u32),
}

impl XfsLogFormat {
    pub(crate) fn from_raw(value: u32) -> Self {
        match value {
            0 => Self::Unknown,
            1 => Self::LinuxLittleEndian,
            2 => Self::LinuxBigEndian,
            3 => Self::IrixBigEndian,
            other => Self::Other(other),
        }
    }

    pub(crate) fn native_u16(self, bytes: &[u8], offset: usize) -> Option<u16> {
        let data = bytes.get(offset..offset + 2)?;
        Some(match self {
            Self::LinuxLittleEndian => u16::from_le_bytes([data[0], data[1]]),
            Self::LinuxBigEndian | Self::IrixBigEndian => u16::from_be_bytes([data[0], data[1]]),
            Self::Unknown | Self::Other(_) => return None,
        })
    }

    pub(crate) fn native_u32(self, bytes: &[u8], offset: usize) -> Option<u32> {
        let data = bytes.get(offset..offset + 4)?;
        Some(match self {
            Self::LinuxLittleEndian => u32::from_le_bytes(data.try_into().ok()?),
            Self::LinuxBigEndian | Self::IrixBigEndian => u32::from_be_bytes(data.try_into().ok()?),
            Self::Unknown | Self::Other(_) => return None,
        })
    }

    pub(crate) fn native_u64(self, bytes: &[u8], offset: usize) -> Option<u64> {
        let data = bytes.get(offset..offset + 8)?;
        Some(match self {
            Self::LinuxLittleEndian => u64::from_le_bytes(data.try_into().ok()?),
            Self::LinuxBigEndian | Self::IrixBigEndian => u64::from_be_bytes(data.try_into().ok()?),
            Self::Unknown | Self::Other(_) => return None,
        })
    }

    pub(crate) fn native_i64(self, bytes: &[u8], offset: usize) -> Option<i64> {
        self.native_u64(bytes, offset).map(|value| value as i64)
    }
}

pub(super) mod header_offset {
    pub const MAGIC: usize = 0;
    pub const CYCLE: usize = 4;
    pub const VERSION: usize = 8;
    pub const DATA_LEN: usize = 12;
    pub const LSN: usize = 16;
    pub const TAIL_LSN: usize = 24;
    pub const CRC: usize = 32;
    pub const PREV_BLOCK: usize = 36;
    pub const NUM_LOGOPS: usize = 40;
    pub const CYCLE_DATA: usize = 44;
    pub const FORMAT: usize = 300;
    pub const FS_UUID: usize = 304;
    pub const ICLOG_SIZE: usize = 320;
}

pub(super) fn be_u16(buf: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([buf[offset], buf[offset + 1]])
}

pub(super) fn be_u32(buf: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
}

pub(super) fn be_u64(buf: &[u8], offset: usize) -> u64 {
    u64::from_be_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
        buf[offset + 4],
        buf[offset + 5],
        buf[offset + 6],
        buf[offset + 7],
    ])
}

pub(super) fn le_u32(buf: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
}
