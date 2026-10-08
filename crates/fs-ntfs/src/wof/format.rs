use super::{Result, WofError};

#[derive(Clone, Copy, Debug)]
pub(crate) enum WofAlgorithm {
    Xpress4k,
    Xpress8k,
    Xpress16k,
    Lzx,
}

impl WofAlgorithm {
    pub(super) fn chunk_size(self) -> u64 {
        match self {
            Self::Xpress4k => 4096,
            Self::Xpress8k => 8192,
            Self::Xpress16k => 16384,
            Self::Lzx => 32768,
        }
    }

    pub(crate) fn from_reparse(data: &[u8]) -> std::io::Result<Option<Self>> {
        parse_reparse(data).map_err(Into::into)
    }
}

fn parse_reparse(data: &[u8]) -> Result<Option<WofAlgorithm>> {
    if word(data, 0)? != 0x8000_0017 {
        return Ok(None);
    }
    if data.len() < 24 {
        return Err(WofError::Invalid("truncated file-provider reparse point"));
    }
    let length = usize::from(u16::from_le_bytes([data[4], data[5]]));
    if length.checked_add(8) != Some(data.len()) || data[6..8] != [0, 0] {
        return Err(WofError::Invalid(
            "invalid reparse length or reserved field",
        ));
    }
    if word(data, 8)? != 1 {
        return Err(WofError::Unsupported("external-info version"));
    }
    if word(data, 12)? != 2 {
        return Err(WofError::Unsupported("external WIM or unknown provider"));
    }
    if word(data, 16)? != 1 {
        return Err(WofError::Unsupported("file-provider version"));
    }
    if data.len() != 24 && (data.len() != 28 || word(data, 24)? != 0) {
        return Err(WofError::Invalid("unknown file-provider payload or flags"));
    }
    Ok(Some(match word(data, 20)? {
        0 => WofAlgorithm::Xpress4k,
        1 => WofAlgorithm::Lzx,
        2 => WofAlgorithm::Xpress8k,
        3 => WofAlgorithm::Xpress16k,
        _ => return Err(WofError::Unsupported("compression algorithm")),
    }))
}

fn word(data: &[u8], offset: usize) -> Result<u32> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or(WofError::Invalid("truncated reparse field"))?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}
