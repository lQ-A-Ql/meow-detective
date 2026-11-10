//! A compressed extent item whose window does not fit its extent is
//! refused as such, and never used as an index (#73).
//!
//! The window check (`offset + num_bytes <= ram_bytes`) sat below the
//! compressed branch, so it ran only for uncompressed extents. For a
//! compressed one `offset` indexes the decoded buffer directly: an offset
//! near `u64::MAX` overflowed that index -- a panic wherever overflow
//! checks are on -- and one merely past the end was blamed on the extent's
//! length.
//!
//! The image is `mkfs.btrfs --rootdir` of a file whose bytes ARE a zlib
//! stream; its extent item is then marked zlib-compressed with a chosen
//! window, in every copy of the leaf, and restamped. `mkfs.btrfs` runs in
//! the harness VM, the one place this suite has btrfs-progs, so the tool is
//! always there and nothing here is conditional on finding it.

use fs_btrfs::btree::{header_offsets, HEADER_SIZE, ITEM_SIZE};
use fs_btrfs::chunk::objectid;
use fs_btrfs::error::Error;
use fs_btrfs::fs::Filesystem;
use fs_btrfs::superblock::Superblock;
use fs_btrfs::tree_write::stamp_checksum;
use fs_btrfs_test_support::{oracle, temp_path};
use fs_core::FileDevice;
use std::sync::Arc;

const SUPERBLOCK: usize = 0x1_0000;
/// `BTRFS_EXTENT_DATA_KEY`.
const EXTENT_DATA: u8 = 108;
// `btrfs_file_extent_item` offsets.
const RAM_BYTES: usize = 8;
const COMPRESSION: usize = 16;
const OFFSET: usize = 37;
const NUM_BYTES: usize = 45;

const PLAIN: usize = 8192;

/// Incompressible bytes, so the zlib stream is as long as the data and the
/// file holding it gets a regular extent rather than an inline one.
fn plain() -> Vec<u8> {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    (0..PLAIN)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}

/// An image whose file's extent is zlib with the window `window(sector,
/// ram_bytes)` returns as (`offset`, `num_bytes`), and the file's inode
/// number. The window is computed from the volume's sector size, because
/// every extent field must be whole sectors (#89) and `mkfs.btrfs` picks
/// the page size of the machine it runs on -- here the harness guest: 4 KiB
/// on x86, 16 KiB on a 16K-page arm64 kernel.
///
/// The image and the directory copied into it both live under the suite's
/// scratch directory inside this repository, because that is the tree the
/// guest running `mkfs.btrfs` can see.
fn image(name: &str, window: impl Fn(u64, u64) -> (u64, u64)) -> (std::path::PathBuf, u64) {
    let dir = std::path::PathBuf::from(temp_path!("comp-window-{name}"));
    let root = dir.join("root");
    std::fs::create_dir_all(&root).unwrap();
    let plain = plain();
    let mut stream = miniz_oxide::deflate::compress_to_vec_zlib(&plain, 6);
    assert!(
        stream.len() > 2 * 4096,
        "fixture: the file must be long enough for a regular extent"
    );
    stream.resize(stream.len().div_ceil(4096) * 4096 + 4096, 0);
    std::fs::write(root.join("file.bin"), &stream).unwrap();
    let img = dir.join("img");
    std::fs::File::create(&img)
        .unwrap()
        .set_len(256 * 1024 * 1024)
        .unwrap();
    let made = oracle("mkfs.btrfs")
        .arg("-f")
        .arg("--rootdir")
        .arg(&root)
        .arg(&img)
        .output();
    assert!(
        made.status.success(),
        "{}",
        String::from_utf8_lossy(&made.stderr)
    );

    let ino = Filesystem::mount(Arc::new(FileDevice::open(&img).unwrap()))
        .unwrap()
        .lookup_path("/file.bin")
        .unwrap()
        .ino;
    let mut bytes = std::fs::read(&img).unwrap();
    let sb = Superblock::parse(&bytes[SUPERBLOCK..SUPERBLOCK + 4096]).unwrap();
    let node = sb.nodesize as usize;
    let sector = u64::from(sb.sectorsize);
    let ram_bytes = (PLAIN as u64).div_ceil(sector) * sector;
    let (offset, num_bytes) = window(sector, ram_bytes);
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
                if u64::from_le_bytes(block[item..item + 8].try_into().unwrap()) != ino
                    || block[item + 8] != EXTENT_DATA
                {
                    continue;
                }
                let e = HEADER_SIZE
                    + u32::from_le_bytes(block[item + 17..item + 21].try_into().unwrap()) as usize;
                block[e + RAM_BYTES..e + RAM_BYTES + 8].copy_from_slice(&ram_bytes.to_le_bytes());
                block[e + COMPRESSION] = 1;
                block[e + OFFSET..e + OFFSET + 8].copy_from_slice(&offset.to_le_bytes());
                block[e + NUM_BYTES..e + NUM_BYTES + 8].copy_from_slice(&num_bytes.to_le_bytes());
                hit = true;
            }
            if hit {
                stamp_checksum(block, &sb);
                patched += 1;
            }
        }
        at += 4096;
    }
    assert!(patched > 0, "fixture: the file's extent item was found");
    std::fs::write(&img, &bytes).unwrap();
    (img, ino)
}

fn read_first_block(img: &std::path::Path, ino: u64) -> Result<Vec<u8>, Error> {
    let fs = Filesystem::mount(Arc::new(FileDevice::open(img).unwrap()))?;
    let mut buf = vec![0u8; 4096];
    fs.read_at(ino, 0, &mut buf).map(|_| buf)
}

#[test]
fn a_compressed_window_that_overflows_is_refused_not_a_panic() {
    // Whole sectors at any sector size up to 64 KiB, and the sum overflows.
    let (img, ino) = image("overflow", |_, _| (!0xFFFF_u64, 0x1_0000));
    let got = std::panic::catch_unwind(|| read_first_block(&img, ino));
    match got {
        Err(_) => panic!("a compressed extent's offset near u64::MAX panicked"),
        Ok(Err(Error::BadSuperblock(m))) => assert!(m.contains("an extent item covers"), "{m}"),
        Ok(other) => panic!(
            "expected the window to be refused, got {:?}",
            other.map(|_| ())
        ),
    }
    let _ = std::fs::remove_dir_all(img.parent().unwrap());
}

#[test]
fn a_compressed_window_past_the_decoded_length_is_refused_by_name() {
    let (img, ino) = image("past", |sector, ram| (ram, sector));
    match read_first_block(&img, ino) {
        Err(Error::BadSuperblock(m)) => assert!(
            m.contains("an extent item covers"),
            "blamed something else: {m}"
        ),
        other => panic!(
            "expected the window to be refused, got {:?}",
            other.map(|_| ())
        ),
    }
    let _ = std::fs::remove_dir_all(img.parent().unwrap());
}

/// Control: a window inside the decoded bytes reads them.
#[test]
fn a_compressed_window_inside_the_extent_reads() {
    let (img, ino) = image("inside", |sector, _| (0, sector));
    let got = read_first_block(&img, ino).expect("a valid compressed window reads");
    assert_eq!(got, plain()[..4096]);
    let _ = std::fs::remove_dir_all(img.parent().unwrap());
}
