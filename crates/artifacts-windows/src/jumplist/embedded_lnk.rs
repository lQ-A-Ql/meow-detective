const LNK_HEADER_SIZE: usize = 76;
const HAS_LINK_TARGET_ID_LIST: u32 = 0x0000_0001;
const HAS_LINK_INFO: u32 = 0x0000_0002;

pub(super) fn extract_lnk_blocks(data: &[u8]) -> Vec<Vec<u8>> {
    let mut blocks = Vec::new();
    let mut offset = 0;
    while offset + 4 < data.len() {
        if is_lnk_header(data, offset) {
            if let Some(size) = lnk_block_size(data, offset) {
                blocks.push(data[offset..offset + size].to_vec());
            }
        }
        offset += 1;
    }
    blocks
}

fn is_lnk_header(data: &[u8], offset: usize) -> bool {
    data.get(offset..offset + 4) == Some(&[0x4c, 0x00, 0x00, 0x00])
        && offset + LNK_HEADER_SIZE <= data.len()
}

fn lnk_block_size(data: &[u8], offset: usize) -> Option<usize> {
    let flags = read_u32(data, offset + 20).unwrap_or(0);
    let mut size = LNK_HEADER_SIZE;
    if flags & HAS_LINK_TARGET_ID_LIST != 0 {
        let id_list_size = read_u16(data, offset + size)? as usize;
        size = size.checked_add(2 + id_list_size)?;
    }
    if flags & HAS_LINK_INFO != 0 {
        let link_info_size = read_u32(data, offset + size)? as usize;
        size = size.checked_add(link_info_size)?;
    }
    (offset.checked_add(size)? <= data.len()).then_some(size)
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        data.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}
