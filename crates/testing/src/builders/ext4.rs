//! Deterministic ext4 byte fixtures shared by filesystem and volume tests.

const BLOCK_SIZE: usize = 4096;
const TOTAL_BLOCKS: usize = 10;

/// Builds a minimal ext4 image containing `test.txt` and
/// `subdir/hello.dat`.
pub fn minimal_ext4_image() -> Vec<u8> {
    let mut image = vec![0u8; TOTAL_BLOCKS * BLOCK_SIZE];
    write_superblock(&mut image, TOTAL_BLOCKS as u32);
    image[BLOCK_SIZE + 0x08..BLOCK_SIZE + 0x0c].copy_from_slice(&2u32.to_le_bytes());

    write_extent_inode(&mut image, 2, 0x41ed, BLOCK_SIZE as u64, 3);
    write_extent_inode(&mut image, 3, 0x81a4, 11, 4);
    write_extent_inode(&mut image, 4, 0x41ed, BLOCK_SIZE as u64, 5);
    write_extent_inode(&mut image, 5, 0x81a4, 13, 6);
    write_fast_symlink(&mut image, 6, b"/usr/bin/perl");

    write_root_directory(&mut image[3 * BLOCK_SIZE..4 * BLOCK_SIZE]);
    image[4 * BLOCK_SIZE..4 * BLOCK_SIZE + 11].copy_from_slice(b"Hello World");
    write_subdirectory(&mut image[5 * BLOCK_SIZE..6 * BLOCK_SIZE]);
    image[6 * BLOCK_SIZE..6 * BLOCK_SIZE + 13].copy_from_slice(b"Hello subdir!");
    image
}

fn write_superblock(image: &mut [u8], total_blocks: u32) {
    let superblock = &mut image[1024..2048];
    superblock[0x00..0x04].copy_from_slice(&16u32.to_le_bytes());
    superblock[0x04..0x08].copy_from_slice(&total_blocks.to_le_bytes());
    superblock[0x14..0x18].copy_from_slice(&0u32.to_le_bytes());
    superblock[0x18..0x1c].copy_from_slice(&2u32.to_le_bytes());
    superblock[0x20..0x24].copy_from_slice(&32768u32.to_le_bytes());
    superblock[0x28..0x2c].copy_from_slice(&16u32.to_le_bytes());
    superblock[0x38..0x3a].copy_from_slice(&0xef53u16.to_le_bytes());
    superblock[0x58..0x5a].copy_from_slice(&256u16.to_le_bytes());
}

const OS_RELEASE: &[u8] =
    b"NAME=\"CentOS Linux\"\nID=\"centos\"\nPRETTY_NAME=\"CentOS Linux 7 (Core)\"\n";

const SHADOW: &[u8] =
    b"root:$6$saltsalt$IVr4ZkQnniyUfgoFSypTZBKHqOP3ApkoDYmbFo9HcgVJ2mzieorJUiB5CH6MDFBWGfgmrBOhof..XdUxzRcUa/:19000:0:99999:7:::\nuser::19001:0:99999:7:::\n";

/// Builds a minimal ext4 image shaped like a Linux system root:
/// `/etc/os-release`, `/etc/fstab`, `/boot/vmlinuz-5.14.0` and `/sbin/init`.
pub fn linux_root_ext4_image() -> Vec<u8> {
    build_linux_root(true, true, false)
}

/// Variant without `/boot` and without `/etc/fstab`: the probe must flag
/// `no-kernel` and `no-fstab`.
pub fn linux_root_ext4_image_without_boot_assets() -> Vec<u8> {
    build_linux_root(false, false, false)
}

/// Variant carrying os-release only at the `/usr/lib/os-release` fallback
/// location (no fstab): exercises the fallback read and `no-fstab`.
pub fn linux_root_ext4_image_usr_lib_os_release() -> Vec<u8> {
    build_linux_root(true, false, true)
}

fn build_linux_root(
    include_boot: bool,
    include_fstab: bool,
    os_release_in_usr_lib: bool,
) -> Vec<u8> {
    const LINUX_BLOCKS: usize = 16;
    let mut image = vec![0u8; LINUX_BLOCKS * BLOCK_SIZE];
    write_superblock(&mut image, LINUX_BLOCKS as u32);
    image[BLOCK_SIZE + 0x08..BLOCK_SIZE + 0x0c].copy_from_slice(&2u32.to_le_bytes());

    write_extent_inode(&mut image, 2, 0x41ed, BLOCK_SIZE as u64, 3); // /
    write_extent_inode(&mut image, 3, 0x41ed, BLOCK_SIZE as u64, 4); // /etc
    write_extent_inode(&mut image, 5, 0x81a4, 12, 6); // fstab
    write_extent_inode(&mut image, 6, 0x41ed, BLOCK_SIZE as u64, 7); // /boot
    write_extent_inode(&mut image, 7, 0x81a4, 4, 8); // vmlinuz-5.14.0
    write_extent_inode(&mut image, 8, 0x41ed, BLOCK_SIZE as u64, 9); // /sbin
    write_extent_inode(&mut image, 9, 0x81a4, 4, 10); // init
    write_extent_inode(&mut image, 10, 0x81a4, SHADOW.len() as u64, 11); // /etc/shadow
    write_extent_inode(&mut image, 11, 0x41ed, BLOCK_SIZE as u64, 12); // /usr
    write_extent_inode(&mut image, 12, 0x41ed, BLOCK_SIZE as u64, 13); // /usr/lib
    if os_release_in_usr_lib {
        write_extent_inode(&mut image, 13, 0x81a4, OS_RELEASE.len() as u64, 14);
        image[14 * BLOCK_SIZE..14 * BLOCK_SIZE + OS_RELEASE.len()].copy_from_slice(OS_RELEASE);
    } else {
        write_extent_inode(&mut image, 4, 0x81a4, OS_RELEASE.len() as u64, 5);
        image[5 * BLOCK_SIZE..5 * BLOCK_SIZE + OS_RELEASE.len()].copy_from_slice(OS_RELEASE);
    }

    let mut root_entries: Vec<(u32, u8, &str)> = vec![(3, 2, "etc")];
    if include_boot {
        root_entries.push((6, 2, "boot"));
    }
    root_entries.push((8, 2, "sbin"));
    if os_release_in_usr_lib {
        root_entries.push((11, 2, "usr"));
    }
    write_dir(
        &mut image[3 * BLOCK_SIZE..4 * BLOCK_SIZE],
        2,
        2,
        &root_entries,
    );

    let mut etc_entries: Vec<(u32, u8, &str)> = Vec::new();
    if !os_release_in_usr_lib {
        etc_entries.push((4, 1, "os-release"));
    }
    if include_fstab {
        etc_entries.push((5, 1, "fstab"));
    }
    etc_entries.push((10, 1, "shadow"));
    write_dir(
        &mut image[4 * BLOCK_SIZE..5 * BLOCK_SIZE],
        3,
        2,
        &etc_entries,
    );
    image[6 * BLOCK_SIZE..6 * BLOCK_SIZE + 12].copy_from_slice(b"# fake fstab");
    image[11 * BLOCK_SIZE..11 * BLOCK_SIZE + SHADOW.len()].copy_from_slice(SHADOW);

    write_dir(
        &mut image[7 * BLOCK_SIZE..8 * BLOCK_SIZE],
        6,
        2,
        &[(7, 1, "vmlinuz-5.14.0")],
    );
    image[8 * BLOCK_SIZE..8 * BLOCK_SIZE + 4].copy_from_slice(b"KERN");

    write_dir(
        &mut image[9 * BLOCK_SIZE..10 * BLOCK_SIZE],
        8,
        2,
        &[(9, 1, "init")],
    );
    image[10 * BLOCK_SIZE..10 * BLOCK_SIZE + 4].copy_from_slice(b"INIT");

    write_dir(
        &mut image[12 * BLOCK_SIZE..13 * BLOCK_SIZE],
        11,
        2,
        &[(12, 2, "lib")],
    );
    write_dir(
        &mut image[13 * BLOCK_SIZE..14 * BLOCK_SIZE],
        12,
        11,
        &[(13, 1, "os-release")],
    );
    image
}

/// Write a single-block directory: `.`/`..` followed by the given entries;
/// the last entry absorbs the rest of the block per ext4 convention.
fn write_dir(block: &mut [u8], self_inode: u32, parent_inode: u32, entries: &[(u32, u8, &str)]) {
    write_directory_entry(block, 0, self_inode, 12, 2, ".");
    write_directory_entry(block, 12, parent_inode, 12, 2, "..");
    let mut offset = 24usize;
    for (index, (inode, file_type, name)) in entries.iter().enumerate() {
        let rec = if index == entries.len() - 1 {
            (block.len() - offset) as u16
        } else {
            (8 + name.len() as u16 + 3) & !3
        };
        write_directory_entry(block, offset, *inode, rec, *file_type, name);
        offset += rec as usize;
    }
}

fn write_extent_inode(image: &mut [u8], inode: usize, mode: u16, size: u64, block: u32) {
    let offset = 2 * BLOCK_SIZE + (inode - 1) * 256;
    let bytes = &mut image[offset..offset + 256];
    bytes[0x00..0x02].copy_from_slice(&mode.to_le_bytes());
    bytes[0x04..0x08].copy_from_slice(&(size as u32).to_le_bytes());
    bytes[0x1c..0x20].copy_from_slice(&8u32.to_le_bytes());
    bytes[0x20..0x24].copy_from_slice(&0x0008_0000u32.to_le_bytes()); // EXT4_EXTENTS_FL
    bytes[0x28..0x2a].copy_from_slice(&0xf30au16.to_le_bytes());
    bytes[0x2a..0x2c].copy_from_slice(&1u16.to_le_bytes());
    bytes[0x2c..0x2e].copy_from_slice(&4u16.to_le_bytes());
    bytes[0x38..0x3a].copy_from_slice(&1u16.to_le_bytes());
    bytes[0x3c..0x40].copy_from_slice(&block.to_le_bytes());
}

fn write_fast_symlink(image: &mut [u8], inode: usize, target: &[u8]) {
    let offset = 2 * BLOCK_SIZE + (inode - 1) * 256;
    let bytes = &mut image[offset..offset + 256];
    bytes[0x00..0x02].copy_from_slice(&0xa1ffu16.to_le_bytes());
    bytes[0x04..0x08].copy_from_slice(&(target.len() as u32).to_le_bytes());
    bytes[0x28..0x28 + target.len()].copy_from_slice(target);
}

fn write_root_directory(block: &mut [u8]) {
    write_directory_entry(block, 0, 2, 12, 2, ".");
    write_directory_entry(block, 12, 2, 12, 2, "..");
    write_directory_entry(block, 24, 3, 24, 1, "test.txt");
    write_directory_entry(block, 48, 4, (BLOCK_SIZE - 48) as u16, 2, "subdir");
}

fn write_subdirectory(block: &mut [u8]) {
    write_directory_entry(block, 0, 4, 12, 2, ".");
    write_directory_entry(block, 12, 2, 12, 2, "..");
    write_directory_entry(block, 24, 5, (BLOCK_SIZE - 24) as u16, 1, "hello.dat");
}

fn write_directory_entry(
    block: &mut [u8],
    offset: usize,
    inode: u32,
    record_length: u16,
    file_type: u8,
    name: &str,
) {
    block[offset..offset + 4].copy_from_slice(&inode.to_le_bytes());
    block[offset + 4..offset + 6].copy_from_slice(&record_length.to_le_bytes());
    block[offset + 6] = name.len() as u8;
    block[offset + 7] = file_type;
    block[offset + 8..offset + 8 + name.len()].copy_from_slice(name.as_bytes());
}
