//! End-to-end exercise of the bootstrap chain over the public API:
//! superblock bytes in, translated physical addresses out.
//!
//! **Necessary but not sufficient.** The superblock and system chunk
//! array below are assembled by this test using the crate's own offset
//! constants, so the test proves the two halves of the crate agree with
//! each other — it cannot prove either agrees with a filesystem the
//! kernel made. Only a fixture produced by a real `mkfs.btrfs` settles
//! that, which is what the cross-validation harness is for. What this
//! file does buy is coverage of the seam the unit tests do not touch: a
//! superblock parsed through the public entry point, its embedded array
//! handed to the chunk map, and a tree-root address translated the way a
//! mount would translate it.

use fs_btrfs::chunk::{block_group, key_type, objectid, ChunkProfile, DISK_KEY_SIZE};
use fs_btrfs::superblock::{
    dev_item_offsets, offsets, ChecksumType, CSUM_SIZE, DEV_ITEM_SIZE, LABEL_SIZE,
    SUPER_INFO_OFFSET, SUPER_INFO_SIZE, UUID_SIZE,
};
use fs_btrfs::{ChunkMap, Error, Superblock};

const FSID: [u8; UUID_SIZE] = [0x3C; UUID_SIZE];
const DEV_UUID: [u8; UUID_SIZE] = [0x7E; UUID_SIZE];
const SECTORSIZE: u32 = 4096;
const STRIPE_LEN: u64 = 64 * 1024;

/// Logical range the one bootstrap SYSTEM chunk covers.
const SYS_LOGICAL: u64 = 0x100_0000;
const SYS_LENGTH: u64 = 0x40_0000;
/// Where that range actually lives on the device.
const SYS_PHYSICAL: u64 = 0x400_0000;

const CHUNK_ROOT: u64 = SYS_LOGICAL + 0x4000;
const ROOT: u64 = SYS_LOGICAL + 0x20_0000;

fn put16(b: &mut [u8], off: usize, v: u16) {
    b[off..off + 2].copy_from_slice(&v.to_le_bytes());
}
fn put32(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}
fn put64(b: &mut [u8], off: usize, v: u64) {
    b[off..off + 8].copy_from_slice(&v.to_le_bytes());
}

/// One `(key, chunk item)` pair: a SINGLE SYSTEM chunk on device 1.
fn sys_chunk_array() -> Vec<u8> {
    let mut a = Vec::new();
    // key
    a.extend_from_slice(&objectid::FIRST_CHUNK_TREE.to_le_bytes());
    a.push(key_type::CHUNK_ITEM);
    a.extend_from_slice(&SYS_LOGICAL.to_le_bytes());
    // chunk item header
    let mut c = vec![0u8; 48 + 32];
    c[0x00..0x08].copy_from_slice(&SYS_LENGTH.to_le_bytes());
    c[0x08..0x10].copy_from_slice(&objectid::EXTENT_TREE.to_le_bytes());
    c[0x10..0x18].copy_from_slice(&STRIPE_LEN.to_le_bytes());
    c[0x18..0x20].copy_from_slice(&block_group::SYSTEM.to_le_bytes());
    c[0x20..0x24].copy_from_slice(&SECTORSIZE.to_le_bytes());
    c[0x24..0x28].copy_from_slice(&SECTORSIZE.to_le_bytes());
    c[0x28..0x2c].copy_from_slice(&SECTORSIZE.to_le_bytes());
    c[0x2c..0x2e].copy_from_slice(&1u16.to_le_bytes());
    c[0x2e..0x30].copy_from_slice(&0u16.to_le_bytes());
    // single stripe
    c[48..56].copy_from_slice(&1u64.to_le_bytes());
    c[56..64].copy_from_slice(&SYS_PHYSICAL.to_le_bytes());
    c[64..80].copy_from_slice(&DEV_UUID);
    a.extend_from_slice(&c);
    assert_eq!(a.len(), DISK_KEY_SIZE + 80);
    a
}

fn superblock_bytes() -> Vec<u8> {
    let mut b = vec![0u8; SUPER_INFO_SIZE];
    b[offsets::MAGIC..offsets::MAGIC + 8].copy_from_slice(b"_BHRfS_M");
    b[offsets::FSID..offsets::FSID + UUID_SIZE].copy_from_slice(&FSID);
    put64(&mut b, offsets::BYTENR, SUPER_INFO_OFFSET);
    put64(&mut b, offsets::GENERATION, 42);
    put64(&mut b, offsets::ROOT, ROOT);
    put64(&mut b, offsets::CHUNK_ROOT, CHUNK_ROOT);
    put64(&mut b, offsets::TOTAL_BYTES, 8 * 1024 * 1024 * 1024);
    put64(&mut b, offsets::BYTES_USED, 64 * 1024 * 1024);
    put64(&mut b, offsets::ROOT_DIR_OBJECTID, 6);
    put64(&mut b, offsets::NUM_DEVICES, 1);
    put32(&mut b, offsets::SECTORSIZE, SECTORSIZE);
    put32(&mut b, offsets::NODESIZE, 16384);
    put32(&mut b, offsets::LEAFSIZE, 16384);
    put32(&mut b, offsets::STRIPESIZE, SECTORSIZE);
    put64(&mut b, offsets::CHUNK_ROOT_GENERATION, 42);
    put16(&mut b, offsets::CSUM_TYPE, ChecksumType::Crc32c.to_raw());
    b[offsets::ROOT_LEVEL] = 1;

    let d = offsets::DEV_ITEM;
    put64(&mut b, d + dev_item_offsets::DEVID, 1);
    put64(
        &mut b,
        d + dev_item_offsets::TOTAL_BYTES,
        8 * 1024 * 1024 * 1024,
    );
    put64(&mut b, d + dev_item_offsets::BYTES_USED, 64 * 1024 * 1024);
    put32(&mut b, d + dev_item_offsets::SECTOR_SIZE, SECTORSIZE);
    b[d + dev_item_offsets::UUID..d + dev_item_offsets::UUID + UUID_SIZE]
        .copy_from_slice(&DEV_UUID);
    b[d + dev_item_offsets::FSID..d + dev_item_offsets::FSID + UUID_SIZE].copy_from_slice(&FSID);
    assert_eq!(d + DEV_ITEM_SIZE, offsets::LABEL);

    let label = b"bootstrap";
    b[offsets::LABEL..offsets::LABEL + label.len()].copy_from_slice(label);
    assert!(label.len() < LABEL_SIZE);

    let array = sys_chunk_array();
    put32(&mut b, offsets::SYS_CHUNK_ARRAY_SIZE, array.len() as u32);
    b[offsets::SYS_CHUNK_ARRAY..offsets::SYS_CHUNK_ARRAY + array.len()].copy_from_slice(&array);

    let digest = ChecksumType::Crc32c.digest(&b[CSUM_SIZE..SUPER_INFO_SIZE]);
    b[..CSUM_SIZE].copy_from_slice(&digest);
    b
}

#[test]
fn superblock_bootstraps_a_usable_address_map() {
    let raw = superblock_bytes();
    let sb = Superblock::parse_at(&raw, SUPER_INFO_OFFSET).expect("superblock should parse");
    assert_eq!(sb.label, "bootstrap");
    assert_eq!(sb.csum_type, ChecksumType::Crc32c);
    assert_eq!(sb.node_uuid(), FSID);
    assert!(!sb.has_dirty_log());
    assert_eq!(sb.sys_chunk_array.len(), DISK_KEY_SIZE + 80);

    let map = ChunkMap::bootstrap(&sb).expect("system chunk array should bootstrap");
    assert_eq!(map.len(), 1);
    assert_eq!(map.chunks()[0].profile().unwrap(), ChunkProfile::Single);
    assert!(map.chunks()[0].is_system());

    // The whole point of the exercise: the chunk tree root is named by a
    // logical address that means nothing until the bootstrap array is
    // loaded.
    let m = map.map(sb.chunk_root).expect("chunk root should be mapped");
    assert_eq!(m.devid, 1);
    assert_eq!(m.physical, SYS_PHYSICAL + (CHUNK_ROOT - SYS_LOGICAL));
    assert_eq!(m.len, SYS_LENGTH - (CHUNK_ROOT - SYS_LOGICAL));

    let m = map.map(sb.root).expect("root tree should be mapped");
    assert_eq!(m.physical, SYS_PHYSICAL + (ROOT - SYS_LOGICAL));

    // The root tree happens to fall inside the bootstrap chunk here; on a
    // real volume it usually does not, and the address is only resolvable
    // after the chunk tree itself has been walked.
    assert!(!map.covers(SYS_LOGICAL + SYS_LENGTH));
    assert!(matches!(
        map.map(SYS_LOGICAL + SYS_LENGTH),
        Err(Error::UnmappedLogical(_))
    ));
}

#[test]
fn a_single_flipped_bit_anywhere_in_the_superblock_is_caught() {
    // Spot-check across the structure, including the tail past every
    // field the parser reads: the checksum spans the whole 4 KiB.
    for at in [
        offsets::GENERATION,
        offsets::SECTORSIZE,
        offsets::LABEL,
        offsets::SYS_CHUNK_ARRAY,
        offsets::SUPER_ROOTS,
        SUPER_INFO_SIZE - 1,
    ] {
        let mut raw = superblock_bytes();
        raw[at] ^= 0x01;
        assert!(
            matches!(Superblock::parse(&raw), Err(Error::ChecksumMismatch { .. })),
            "a flipped bit at {at:#x} should have failed the checksum"
        );
    }
}
