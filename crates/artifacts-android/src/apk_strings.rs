use super::apk::{chunk_at, read_u16, read_u32, read_u8, ApkInspectError, RES_STRING_POOL_TYPE};

pub(crate) struct StringPool<'a> {
    bytes: &'a [u8],
    offset: usize,
    end: usize,
    offsets: Vec<u32>,
    strings_start: usize,
    utf8: bool,
}

impl<'a> StringPool<'a> {
    pub(crate) fn parse(bytes: &'a [u8], offset: usize) -> Result<Self, ApkInspectError> {
        let chunk = chunk_at(bytes, offset)?;
        if chunk.chunk_type != RES_STRING_POOL_TYPE || chunk.header_size < 28 {
            return Err(ApkInspectError::Resources(
                "invalid string pool".to_string(),
            ));
        }
        let string_count = read_u32(bytes, offset + 8)? as usize;
        let flags = read_u32(bytes, offset + 16)?;
        let strings_start = read_u32(bytes, offset + 20)? as usize;
        let table_start = offset + chunk.header_size;
        let table_end = table_start
            .checked_add(string_count.saturating_mul(4))
            .ok_or_else(|| ApkInspectError::Resources("string offset overflow".to_string()))?;
        if table_end > chunk.end || strings_start >= chunk.end - offset {
            return Err(ApkInspectError::Resources(
                "string pool is truncated".to_string(),
            ));
        }
        let mut offsets = Vec::with_capacity(string_count);
        for index in 0..string_count {
            offsets.push(read_u32(bytes, table_start + index * 4)?);
        }
        Ok(Self {
            bytes,
            offset,
            end: chunk.end,
            offsets,
            strings_start,
            utf8: flags & 0x0000_0100 != 0,
        })
    }

    pub(crate) fn get(&self, index: u32) -> Result<String, ApkInspectError> {
        let offset = self
            .offsets
            .get(index as usize)
            .ok_or_else(|| ApkInspectError::Resources("string index is invalid".to_string()))?;
        let start = self.offset + self.strings_start + *offset as usize;
        if start >= self.end {
            return Err(ApkInspectError::Resources(
                "string offset is outside the pool".to_string(),
            ));
        }
        if self.utf8 {
            return decode_utf8_string(self.bytes, start, self.end);
        }
        decode_utf16_string(self.bytes, start, self.end)
    }
}

fn decode_utf8_string(bytes: &[u8], start: usize, end: usize) -> Result<String, ApkInspectError> {
    let (_, after_chars) = decode_utf8_length(bytes, start, end)?;
    let (byte_length, data_start) = decode_utf8_length(bytes, after_chars, end)?;
    let data_end = data_start
        .checked_add(byte_length)
        .ok_or_else(|| ApkInspectError::Resources("UTF-8 string overflow".to_string()))?;
    if data_end >= end || bytes[data_end] != 0 {
        return Err(ApkInspectError::Resources(
            "UTF-8 string is truncated".to_string(),
        ));
    }
    String::from_utf8(bytes[data_start..data_end].to_vec())
        .map_err(|_| ApkInspectError::Resources("invalid UTF-8 string".to_string()))
}

fn decode_utf16_string(bytes: &[u8], start: usize, end: usize) -> Result<String, ApkInspectError> {
    let (units, data_start) = decode_utf16_length(bytes, start, end)?;
    let byte_length = units
        .checked_mul(2)
        .ok_or_else(|| ApkInspectError::Resources("UTF-16 string overflow".to_string()))?;
    let data_end = data_start
        .checked_add(byte_length)
        .ok_or_else(|| ApkInspectError::Resources("UTF-16 string overflow".to_string()))?;
    if data_end + 2 > end || read_u16(bytes, data_end)? != 0 {
        return Err(ApkInspectError::Resources(
            "UTF-16 string is truncated".to_string(),
        ));
    }
    let units = (0..units)
        .map(|index| read_u16(bytes, data_start + index * 2))
        .collect::<Result<Vec<_>, _>>()?;
    String::from_utf16(&units)
        .map_err(|_| ApkInspectError::Resources("invalid UTF-16 string".to_string()))
}

fn decode_utf8_length(
    bytes: &[u8],
    offset: usize,
    end: usize,
) -> Result<(usize, usize), ApkInspectError> {
    let first = read_u8(bytes, offset)?;
    if first & 0x80 == 0 {
        return Ok((first as usize, offset + 1));
    }
    if offset + 1 >= end {
        return Err(ApkInspectError::Resources(
            "string length is truncated".to_string(),
        ));
    }
    let second = read_u8(bytes, offset + 1)?;
    Ok((((first as usize & 0x7f) << 8) | second as usize, offset + 2))
}

fn decode_utf16_length(
    bytes: &[u8],
    offset: usize,
    end: usize,
) -> Result<(usize, usize), ApkInspectError> {
    let first = read_u16(bytes, offset)?;
    if first & 0x8000 == 0 {
        return Ok((first as usize, offset + 2));
    }
    if offset + 3 >= end {
        return Err(ApkInspectError::Resources(
            "string length is truncated".to_string(),
        ));
    }
    let second = read_u16(bytes, offset + 2)?;
    Ok((
        ((first as usize & 0x7fff) << 16) | second as usize,
        offset + 4,
    ))
}
