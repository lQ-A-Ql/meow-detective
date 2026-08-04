//! Deterministic F2FS byte fixtures shared by filesystem and volume tests.

const BLOCK_SIZE: usize = 4096;
const BLOCK_COUNT: usize = 4100;
const CHECKPOINT_BLOCK: usize = 512;
const NAT_BLOCK: usize = 2560;
const MAIN_BLOCK: usize = 4096;
const F2FS_MAGIC: u32 = 0xf2f5_2010;

pub const ROOT_INODE: u32 = 3;
pub const FILE_INODE: u32 = 4;

pub fn minimal_f2fs_image() -> Vec<u8> {
    let mut image = vec![0u8; BLOCK_COUNT * BLOCK_SIZE];
    write_superblock(&mut image, 1024);
    write_superblock(&mut image, BLOCK_SIZE + 1024);
    write_checkpoint_pack(&mut image, CHECKPOINT_BLOCK, 1);
    write_checkpoint_pack(&mut image, CHECKPOINT_BLOCK + 512, 2);
    write_nat_entry(&mut image, ROOT_INODE, ROOT_INODE, MAIN_BLOCK as u32);
    write_nat_entry(&mut image, FILE_INODE, FILE_INODE, MAIN_BLOCK as u32 + 1);
    write_inode(
        block_mut(&mut image, MAIN_BLOCK),
        ROOT_INODE,
        0x41ed,
        BLOCK_SIZE as u64,
        MAIN_BLOCK as u32 + 2,
    );
    write_inode(
        block_mut(&mut image, MAIN_BLOCK + 1),
        FILE_INODE,
        0x81a4,
        10,
        MAIN_BLOCK as u32 + 3,
    );
    write_directory(block_mut(&mut image, MAIN_BLOCK + 2));
    block_mut(&mut image, MAIN_BLOCK + 3)[..10].copy_from_slice(b"Hello F2FS");
    image
}

fn write_superblock(image: &mut [u8], offset: usize) {
    let bytes = &mut image[offset..offset + 2184];
    write_u32(bytes, 0, F2FS_MAGIC);
    write_u16(bytes, 4, 1);
    write_u16(bytes, 6, 15);
    write_u32(bytes, 8, 9);
    write_u32(bytes, 12, 3);
    write_u32(bytes, 16, 12);
    write_u32(bytes, 20, 9);
    write_u32(bytes, 24, 1);
    write_u32(bytes, 28, 1);
    write_u64(bytes, 36, BLOCK_COUNT as u64);
    write_u32(bytes, 44, 1);
    write_u32(bytes, 48, 8);
    write_u32(bytes, 52, 2);
    write_u32(bytes, 56, 2);
    write_u32(bytes, 60, 2);
    write_u32(bytes, 64, 1);
    write_u32(bytes, 68, 1);
    write_u32(bytes, 72, CHECKPOINT_BLOCK as u32);
    write_u32(bytes, 76, CHECKPOINT_BLOCK as u32);
    write_u32(bytes, 80, 1536);
    write_u32(bytes, 84, NAT_BLOCK as u32);
    write_u32(bytes, 88, 3584);
    write_u32(bytes, 92, MAIN_BLOCK as u32);
    write_u32(bytes, 96, ROOT_INODE);
    write_u32(bytes, 100, 1);
    write_u32(bytes, 104, 2);
}

fn write_checkpoint_pack(image: &mut [u8], start_block: usize, version: u64) {
    let mut checkpoint = vec![0u8; BLOCK_SIZE];
    write_u64(&mut checkpoint, 0, version);
    write_u32(&mut checkpoint, 132, 1);
    write_u32(&mut checkpoint, 136, 8);
    write_u32(&mut checkpoint, 140, 1);
    write_u32(&mut checkpoint, 144, 2);
    write_u32(&mut checkpoint, 148, 2);
    write_u32(&mut checkpoint, 152, 5);
    write_u32(&mut checkpoint, 156, 0);
    write_u32(&mut checkpoint, 160, 1);
    write_u32(&mut checkpoint, 164, (BLOCK_SIZE - 4) as u32);
    let checksum = f2fs_crc32(F2FS_MAGIC, &checkpoint[..BLOCK_SIZE - 4]);
    write_u32(&mut checkpoint, BLOCK_SIZE - 4, checksum);
    block_mut(image, start_block).copy_from_slice(&checkpoint);
    block_mut(image, start_block + 7).copy_from_slice(&checkpoint);
}

fn write_nat_entry(image: &mut [u8], nid: u32, inode: u32, block: u32) {
    let offset = nid as usize * 9;
    let nat = block_mut(image, NAT_BLOCK);
    write_u32(nat, offset + 1, inode);
    write_u32(nat, offset + 5, block);
}

fn write_inode(bytes: &mut [u8], nid: u32, mode: u16, size: u64, data_block: u32) {
    write_u16(bytes, 0, mode);
    write_u64(bytes, 16, size);
    write_u64(bytes, 24, 2);
    write_u32(bytes, 360, data_block);
    write_u32(bytes, 4072, nid);
    write_u32(bytes, 4076, nid);
    write_u64(bytes, 4084, 2);
}

fn write_directory(bytes: &mut [u8]) {
    bytes[0] = 0x0f;
    write_dentry(bytes, 0, ROOT_INODE, 2, ".");
    write_dentry(bytes, 1, ROOT_INODE, 2, "..");
    write_dentry(bytes, 2, FILE_INODE, 1, "hello.txt");
}

fn write_dentry(bytes: &mut [u8], slot: usize, inode: u32, file_type: u8, name: &str) {
    let entry_offset = 30 + slot * 11;
    write_u32(bytes, entry_offset + 4, inode);
    write_u16(bytes, entry_offset + 8, name.len() as u16);
    bytes[entry_offset + 10] = file_type;
    let name_offset = 2384 + slot * 8;
    bytes[name_offset..name_offset + name.len()].copy_from_slice(name.as_bytes());
}

fn block_mut(image: &mut [u8], block: usize) -> &mut [u8] {
    &mut image[block * BLOCK_SIZE..(block + 1) * BLOCK_SIZE]
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

fn f2fs_crc32(mut crc: u32, bytes: &[u8]) -> u32 {
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    crc
}
