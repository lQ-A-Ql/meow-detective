use super::LogReplayError;

#[derive(Debug, Clone)]
pub(super) struct Header {
    pub bytes: [u8; 512],
    pub primary: u32,
    pub secondary: u32,
    pub kind: u32,
    pub root: u32,
    pub bins: usize,
    pub major: u32,
    pub minor: u32,
    pub timestamp: u64,
    pub checksum_valid: bool,
}
impl Header {
    pub fn read(data: &[u8], max_bytes: usize) -> Result<Self, LogReplayError> {
        let bytes: [u8; 512] = data
            .get(..512)
            .and_then(|b| b.try_into().ok())
            .ok_or(LogReplayError::Truncated)?;
        if &bytes[..4] != b"regf" {
            return Err(LogReplayError::Header);
        }
        let major = u32_at(&bytes, 20)?;
        let minor = u32_at(&bytes, 24)?;
        let bins = u32_at(&bytes, 40)? as usize;
        if major != 1
            || !(3..=6).contains(&minor)
            || u32_at(&bytes, 32)? != 1
            || u32_at(&bytes, 44)? != 1
        {
            return Err(LogReplayError::Unsupported);
        }
        if bins < 4096
            || !bins.is_multiple_of(4096)
            || bins.checked_add(4096).is_none_or(|n| n > max_bytes)
        {
            return Err(LogReplayError::Bounds);
        }
        let checksum_valid = checksum(&bytes) == u32_at(&bytes, 508)?;
        Ok(Self {
            primary: u32_at(&bytes, 4)?,
            secondary: u32_at(&bytes, 8)?,
            kind: u32_at(&bytes, 28)?,
            root: u32_at(&bytes, 36)?,
            bins,
            major,
            minor,
            timestamp: u64_at(&bytes, 12)?,
            bytes,
            checksum_valid,
        })
    }
    pub fn dirty(&self) -> bool {
        !self.checksum_valid || self.primary != self.secondary
    }
    pub fn matches(&self, other: &Self) -> bool {
        if self.major != other.major
            || self.minor != other.minor
            || (self.checksum_valid && self.root != other.root)
        {
            return false;
        }
        let base_guid = &self.bytes[112..128];
        let log_guid = &other.bytes[112..128];
        if self.bytes[164..168] == *b"rmtm"
            && other.bytes[164..168] == *b"rmtm"
            && base_guid.iter().any(|b| *b != 0)
            && base_guid != log_guid
        {
            return false;
        }
        let base_name = filename(&self.bytes);
        let log_name = filename(&other.bytes);
        base_name.is_empty() || log_name.is_empty() || base_name == log_name
    }
}
fn filename(bytes: &[u8; 512]) -> String {
    let units: Vec<_> = bytes[48..112]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .take_while(|u| *u != 0)
        .collect();
    String::from_utf16_lossy(&units)
        .replace('/', "\\")
        .rsplit('\\')
        .next()
        .unwrap_or("")
        .to_lowercase()
}
pub(super) fn u32_at(bytes: &[u8], at: usize) -> Result<u32, LogReplayError> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .and_then(|b| b.try_into().ok())
            .ok_or(LogReplayError::Truncated)?,
    ))
}
pub(super) fn u64_at(bytes: &[u8], at: usize) -> Result<u64, LogReplayError> {
    Ok(u64::from_le_bytes(
        bytes
            .get(at..at + 8)
            .and_then(|b| b.try_into().ok())
            .ok_or(LogReplayError::Truncated)?,
    ))
}
pub(super) fn checksum(bytes: &[u8]) -> u32 {
    let sum = bytes[..508]
        .as_chunks::<4>()
        .0
        .iter()
        .fold(0, |sum, b| sum ^ u32::from_le_bytes(*b));
    match sum {
        0 => 1,
        u32::MAX => u32::MAX - 1,
        _ => sum,
    }
}
pub(super) fn forward(sequence: u32, from: u32) -> bool {
    sequence.wrapping_sub(from) <= 0x7fff_ffff
}
