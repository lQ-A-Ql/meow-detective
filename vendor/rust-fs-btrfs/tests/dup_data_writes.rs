//! A write into a `nodatacow` file on a `-d dup` filesystem lands on both
//! copies of the data (#71).
//!
//! `write_at` wrote copy 0 only. This driver reads copy 0, so it read back
//! what it wrote and was satisfied; the other copy kept the old bytes, and
//! a nodatasum file has no checksum to say which is current. A single-disk
//! `mkfs.btrfs -d dup` -- an ordinary laptop choice -- mounts read-write,
//! so no pool is needed to get there.
//!
//! The image is `mkfs.btrfs --rootdir` of one 64 KiB file, the file's
//! inode is marked nodatacow and nodatasum in every copy of its fs-tree
//! leaf (restamped), and the outcome is read from the raw image: the new
//! pattern must be there twice and the old one nowhere. `mkfs.btrfs` runs
//! in the harness VM, which is where this suite's btrfs-progs lives, so
//! there is no missing tool to step around.

use fs_btrfs::btree::{header_offsets, HEADER_SIZE, ITEM_SIZE};
use fs_btrfs::chunk::objectid;
use fs_btrfs::fs::Filesystem;
use fs_btrfs::superblock::Superblock;
use fs_btrfs::tree_write::stamp_checksum;
use fs_btrfs::write::{INODE_NODATACOW, INODE_NODATASUM};
use fs_btrfs_test_support::{oracle, temp_path};
use fs_core::{BlockDevice, FileDevice};
use std::sync::Arc;

const SUPERBLOCK: usize = 0x1_0000;
const LEN: usize = 64 * 1024;
/// `btrfs_inode_item.flags`.
const INODE_FLAGS: usize = 64;

fn pattern(seed: u8) -> Vec<u8> {
    (0..LEN)
        .map(|i| (i as u32).wrapping_mul(2_654_435_761).to_le_bytes()[i % 4] ^ seed)
        .collect()
}

fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .step_by(512)
        .filter(|w| *w == needle)
        .count()
}

#[test]
fn a_nodatacow_write_on_a_dup_filesystem_reaches_both_copies() {
    // Inside the repository, because `mkfs.btrfs` reads the image from the
    // harness VM, which sees this tree and nothing else of the host.
    let dir = std::path::PathBuf::from(temp_path!("dup-data"));
    let root = dir.join("root");
    std::fs::create_dir_all(&root).unwrap();
    let old = pattern(0x11);
    let new = pattern(0x77);
    std::fs::write(root.join("file.bin"), &old).unwrap();
    let img = dir.join("img");
    std::fs::File::create(&img)
        .unwrap()
        .set_len(256 * 1024 * 1024)
        .unwrap();
    let made = oracle("mkfs.btrfs")
        .args(["-f", "-d", "dup", "-m", "dup", "--rootdir"])
        .arg(&root)
        .arg(&img)
        .output();
    assert!(
        made.status.success(),
        "{}",
        String::from_utf8_lossy(&made.stderr)
    );

    let ino = {
        let dev = FileDevice::open(&img).unwrap();
        let fs = Filesystem::mount(Arc::new(dev)).unwrap();
        fs.lookup_path("/file.bin").unwrap().ino
    };

    // Mark the inode nodatacow + nodatasum in every copy of its leaf.
    let mut bytes = std::fs::read(&img).unwrap();
    let sb = Superblock::parse(&bytes[SUPERBLOCK..SUPERBLOCK + 4096]).unwrap();
    let node = sb.nodesize as usize;
    let mut patched = 0;
    let mut at = 0;
    while at + node <= bytes.len() {
        let block = &mut bytes[at..at + node];
        let owner = u64::from_le_bytes(
            block[header_offsets::OWNER..header_offsets::OWNER + 8]
                .try_into()
                .unwrap(),
        );
        if block[header_offsets::FSID..header_offsets::FSID + 16] == sb.fsid
            && owner == objectid::FS_TREE
            && block[header_offsets::LEVEL] == 0
        {
            let nritems = u32::from_le_bytes(
                block[header_offsets::NRITEMS..header_offsets::NRITEMS + 4]
                    .try_into()
                    .unwrap(),
            );
            let mut hit = false;
            for i in 0..nritems as usize {
                let item = HEADER_SIZE + i * ITEM_SIZE;
                if item + ITEM_SIZE > node {
                    break;
                }
                let key_ino = u64::from_le_bytes(block[item..item + 8].try_into().unwrap());
                if key_ino == ino && block[item + 8] == 1 {
                    let off = HEADER_SIZE
                        + u32::from_le_bytes(block[item + 17..item + 21].try_into().unwrap())
                            as usize;
                    let flags = u64::from_le_bytes(
                        block[off + INODE_FLAGS..off + INODE_FLAGS + 8]
                            .try_into()
                            .unwrap(),
                    );
                    block[off + INODE_FLAGS..off + INODE_FLAGS + 8].copy_from_slice(
                        &(flags | INODE_NODATACOW | INODE_NODATASUM).to_le_bytes(),
                    );
                    hit = true;
                }
            }
            if hit {
                stamp_checksum(block, &sb);
                patched += 1;
            }
        }
        at += 4096;
    }
    assert_eq!(
        patched, 2,
        "fixture: `-m dup` keeps two copies of the fs-tree leaf"
    );
    std::fs::write(&img, &bytes).unwrap();
    assert_eq!(
        count(&bytes, &old[..512]),
        2,
        "fixture: `-d dup` stores the file twice"
    );

    {
        let dev = FileDevice::open_rw(&img).unwrap();
        let fs = Filesystem::mount_rw(Arc::new(dev) as Arc<dyn BlockDevice>).unwrap();
        assert_eq!(fs.write_at(ino, 0, &new).unwrap(), LEN);
    }

    let after = std::fs::read(&img).unwrap();
    assert_eq!(
        (count(&after, &new[..512]), count(&after, &old[..512])),
        (2, 0),
        "(copies of the new data, copies of the old): a dup data write must replace both"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
