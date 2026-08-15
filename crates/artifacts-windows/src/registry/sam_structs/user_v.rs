use binread::BinRead;

#[derive(BinRead, Debug)]
#[br(little)]
pub(crate) struct UserVRaw {
    _pad1: [u8; 12],
    name_offset: u32,
    name_length: u32,
    _pad2: u32,
    full_name_offset: u32,
    full_name_length: u32,
    _pad3: u32,
    comment_offset: u32,
    comment_length: u32,
    _pad4: u32,
    home_dir_offset: u32,
    home_dir_length: u32,
    _pad5: u32,
    profile_path_offset: u32,
    profile_path_length: u32,
    _pad6: u32,
    script_path_offset: u32,
    script_path_length: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct SamUserProfile {
    pub username: String,
    pub full_name: String,
    pub comment: String,
    pub home_dir: String,
    pub profile_path: String,
    pub script_path: String,
}

pub(crate) fn parse_username_from_v_record(data: &[u8]) -> Option<String> {
    if data.len() < 0x14 {
        return None;
    }
    let offset = u32::from_le_bytes(data.get(0x0c..0x10)?.try_into().ok()?) as usize;
    let length = u32::from_le_bytes(data.get(0x10..0x14)?.try_into().ok()?) as usize;
    decode_utf16_field(data, offset, length, 256)
}

pub(crate) fn parse_user_v(data: &[u8]) -> Option<SamUserProfile> {
    let mut cursor = std::io::Cursor::new(data);
    let raw = UserVRaw::read(&mut cursor).ok()?;
    Some(SamUserProfile {
        username: extract_utf16le_at(data, raw.name_offset, raw.name_length).unwrap_or_default(),
        full_name: extract_utf16le_at(data, raw.full_name_offset, raw.full_name_length)
            .unwrap_or_default(),
        comment: extract_utf16le_at(data, raw.comment_offset, raw.comment_length)
            .unwrap_or_default(),
        home_dir: extract_utf16le_at(data, raw.home_dir_offset, raw.home_dir_length)
            .unwrap_or_default(),
        profile_path: extract_utf16le_at(data, raw.profile_path_offset, raw.profile_path_length)
            .unwrap_or_default(),
        script_path: extract_utf16le_at(data, raw.script_path_offset, raw.script_path_length)
            .unwrap_or_default(),
    })
}

fn extract_utf16le_at(data: &[u8], offset: u32, length: u32) -> Option<String> {
    decode_utf16_field(data, offset as usize, length as usize, 512)
}

fn decode_utf16_field(
    data: &[u8],
    offset: usize,
    length: usize,
    maximum_length: usize,
) -> Option<String> {
    if length == 0 || length > maximum_length {
        return None;
    }
    let bytes = data.get(offset..offset.checked_add(length)?)?;
    if bytes.len() < 2 || !bytes.len().is_multiple_of(2) {
        return None;
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    let value = String::from_utf16_lossy(&units);
    let trimmed = value.trim_end_matches('\0');
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}
