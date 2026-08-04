//! Deterministic EROFS byte fixtures shared by filesystem tests.

const BLOCK_SIZE: usize = 4096;
const BLOCK_COUNT: usize = 16;
const META_BLOCK: usize = 2;
const ROOT_NID: u64 = 1;
const FILE_NID: u64 = 2;
const EROFS_MAGIC: u32 = 0xe0f5_e1e2;

pub fn minimal_erofs_image() -> Vec<u8> {
    let mut image = vec![0u8; BLOCK_COUNT * BLOCK_SIZE];
    write_superblock(&mut image);
    write_compact_inode(inode_mut(&mut image, ROOT_NID), ROOT_NID, 0x41ed, 4096, 3);
    write_compact_inode(inode_mut(&mut image, FILE_NID), FILE_NID, 0x81a4, 12, 4);
    write_directory(block_mut(&mut image, 3));
    block_mut(&mut image, 4)[..12].copy_from_slice(b"Hello EROFS!");
    image
}

fn write_superblock(image: &mut [u8]) {
    let bytes = &mut image[1024..1024 + 144];
    write_u32(bytes, 0, EROFS_MAGIC);
    bytes[12] = 12;
    write_u16(bytes, 14, ROOT_NID as u16);
    write_u32(bytes, 36, BLOCK_COUNT as u32);
    write_u32(bytes, 40, META_BLOCK as u32);
}

fn write_compact_inode(bytes: &mut [u8], nid: u64, mode: u16, size: u32, block: u32) {
    write_u16(bytes, 0, 0);
    write_u16(bytes, 4, mode);
    write_u32(bytes, 8, size);
    write_u32(bytes, 16, block);
    write_u32(bytes, 20, nid as u32);
}

fn write_directory(bytes: &mut [u8]) {
    write_dirent(bytes, 0, ROOT_NID, 36, 2);
    write_dirent(bytes, 12, ROOT_NID, 37, 2);
    write_dirent(bytes, 24, FILE_NID, 39, 1);
    bytes[36..37].copy_from_slice(b".");
    bytes[37..39].copy_from_slice(b"..");
    bytes[39..48].copy_from_slice(b"hello.txt");
}

fn write_dirent(bytes: &mut [u8], offset: usize, nid: u64, name_offset: u16, file_type: u8) {
    write_u64(bytes, offset, nid);
    write_u16(bytes, offset + 8, name_offset);
    bytes[offset + 10] = file_type;
}

fn block_mut(image: &mut [u8], block: usize) -> &mut [u8] {
    let offset = block * BLOCK_SIZE;
    &mut image[offset..offset + BLOCK_SIZE]
}

fn inode_mut(image: &mut [u8], nid: u64) -> &mut [u8] {
    let offset = META_BLOCK * BLOCK_SIZE + usize::try_from(nid * 32).expect("fixture nid");
    &mut image[offset..offset + 32]
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
