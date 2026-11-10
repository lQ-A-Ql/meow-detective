//! A compressed stream that decodes short is refused anywhere but the
//! file's last extent (#189).
//!
//! `decompress` pads a short decode with zeros up to `ram_bytes`. That is
//! right for the last extent of a file, where the tail of the final sector
//! holds nothing. Anywhere else it turns a truncated or damaged stream into
//! a run of zeros — which is legitimate file content, so a caller cannot
//! tell it from data. Every other decode failure in that module is
//! reported; this one was not, and it is the one that hides damage.
//!
//! THE IMAGE IS THE LZO FIXTURE, on a copy this test is free to damage.
//! `test-disks/btrfs-comp-lzo.img` is written by the kernel through a
//! `compress=lzo` mount in the harness guest (`chore fixtures`), which is
//! the only way to get a compressed extent — `mkfs.btrfs --rootdir` does
//! not compress. Its `/big.txt` is 1.7 MiB of text, so it spans many
//! compressed extents and its first one is not its last.
//!
//! That extent's LZO stream then has its length header lowered so it ends
//! after one segment: the decoder finishes cleanly and hands back a
//! fraction of the bytes the item promises, which is what a truncated or
//! damaged stream looks like from the reader's side. Truncating the
//! *input* instead is caught by the framing — this is the case that is
//! not.
//!
//! The inode is marked `nodatasum` for the same reason the write-side
//! fixtures do: the checksums cover the bytes on disk, and this edits
//! them.
//!
//! Nothing here skips. The fixture is built by `chore fixtures` and a
//! missing one fails the test that wanted it, naming the task that
//! builds it — this suite used to make its own image with `sudo mount -o
//! loop` on the host and return early wherever that was not possible,
//! which is a pass that has checked nothing.

use fs_btrfs::btree::{header_offsets, HEADER_SIZE, ITEM_SIZE};
use fs_btrfs::chunk::objectid;
use fs_btrfs::fs::Filesystem;
use fs_btrfs::superblock::Superblock;
use fs_btrfs::tree_write::stamp_checksum;
use fs_btrfs::write::{INODE_NODATACOW, INODE_NODATASUM};
use fs_btrfs_test_support::{fixture, le64, sha256_hex, temp_path, Image};
use fs_core::{BlockRead, FileDevice};
use std::sync::Arc;

const SUPERBLOCK: usize = 0x1_0000;
/// The fixture's large file: text, so LZO keeps it, and long enough to
/// need many compression units — so its first extent is not its last.
const FILE: &str = "/big.txt";
const DISK_BYTENR: usize = 21;
const INODE_FLAGS: usize = 64;
const INODE_ITEM_KEY: u8 = 1;
/// The LZO framing's first four bytes: how much of the extent is stream.
const LZO_LEN: usize = 4;
const COMPRESSION: usize = 16;
const EXTENT_DATA_KEY: u8 = 108;

/// A writable copy of the LZO fixture, under this run's scratch
/// directory. The fixture itself is an input to the whole suite and is
/// never edited in place.
fn image(name: &str) -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(temp_path!("short-decode-{name}"));
    std::fs::create_dir_all(&dir).unwrap();
    let img = dir.join("img");
    std::fs::copy(fixture("btrfs-comp-lzo.img"), &img).unwrap();
    img
}

/// What the kernel says `FILE` holds: its size and its SHA-256, from the
/// manifest the guest wrote beside the image.
fn expected() -> (u64, String) {
    let manifest = std::fs::read_to_string(fixture("btrfs-comp-lzo.manifest")).unwrap();
    for line in manifest.lines() {
        let mut fields = line.split('\t');
        if fields.next() == Some(FILE) {
            let size = fields.next().unwrap().parse().unwrap();
            return (size, fields.next().unwrap().to_string());
        }
    }
    panic!("the LZO fixture's manifest does not name {FILE}:\n{manifest}");
}

/// Apply `edit(key_type, key_offset, body)` to `ino`'s items in every
/// fs-tree leaf copy, and say how many leaves were touched.
fn edit_items(
    img: &std::path::Path,
    ino: u64,
    mut edit: impl FnMut(u8, u64, &mut [u8]) -> bool,
) -> usize {
    // A BLOCK AT A TIME, read and written back only where something
    // changed: this fixture is 600 MiB, two tests run at once, and the
    // harness guest has four gigabytes. See `Image`.
    let image = Image::open(img);
    let head = image.read_at(SUPERBLOCK as u64, 4096);
    let sb = Superblock::parse(&head).unwrap();
    let node = sb.nodesize as usize;
    let mut patched = 0;
    let mut block_buf = vec![0u8; node];
    let mut at = 0u64;
    while image.try_read_at(at, &mut block_buf) {
        let here = at;
        at += 4096;
        let block = &mut block_buf[..];
        if block[header_offsets::FSID..header_offsets::FSID + 16] != sb.fsid
            || le64(block, header_offsets::OWNER) != objectid::FS_TREE
            || block[header_offsets::LEVEL] != 0
        {
            continue;
        }
        let nritems = u32::from_le_bytes(
            block[header_offsets::NRITEMS..header_offsets::NRITEMS + 4]
                .try_into()
                .unwrap(),
        );
        let mut hit = false;
        for i in 0..nritems as usize {
            let item = HEADER_SIZE + i * ITEM_SIZE;
            if le64(block, item) != ino {
                continue;
            }
            let key_type = block[item + 8];
            let key_offset = le64(block, item + 9);
            let off = HEADER_SIZE
                + u32::from_le_bytes(block[item + 17..item + 21].try_into().unwrap()) as usize;
            let size = u32::from_le_bytes(block[item + 21..item + 25].try_into().unwrap()) as usize;
            hit |= edit(key_type, key_offset, &mut block[off..off + size]);
        }
        if hit {
            stamp_checksum(block, &sb);
            write_window(img, here, block);
            patched += 1;
        }
    }
    patched
}

/// Put `bytes` back at `offset` in `img`.
fn write_window(img: &std::path::Path, offset: u64, bytes: &[u8]) {
    use std::os::unix::fs::FileExt;
    std::fs::File::options()
        .write(true)
        .open(img)
        .unwrap_or_else(|e| panic!("opening {} to write: {e}", img.display()))
        .write_all_at(bytes, offset)
        .unwrap_or_else(|e| panic!("writing {} at {offset}: {e}", img.display()));
}

fn mount(img: &std::path::Path) -> Filesystem {
    Filesystem::mount(Arc::new(FileDevice::open(img).unwrap()) as Arc<dyn BlockRead>).unwrap()
}

/// Control: the file as the kernel wrote it reads back, and it really is
/// compressed and in more than one extent — otherwise the test below is
/// about nothing.
#[test]
fn the_compressed_file_as_made_reads_back() {
    let img = image("control");
    let (size, sha256) = expected();
    let fs = mount(&img);
    let ino = fs.lookup_path(FILE).unwrap().ino;
    let mut compressed = 0usize;
    edit_items(&img, ino, |key_type, _, body| {
        if key_type == EXTENT_DATA_KEY && body[COMPRESSION] != 0 {
            compressed += 1;
        }
        false
    });
    assert!(
        compressed >= 2,
        "the fixture holds {compressed} compressed extents of {FILE}, so none of them \
         is a non-final one"
    );
    let read = mount(&img).read_file(ino).expect("an ordinary read");
    assert_eq!(read.len() as u64, size);
    assert_eq!(
        sha256_hex(&read),
        sha256,
        "{FILE} did not read back what the kernel wrote to it"
    );
    let _ = std::fs::remove_dir_all(img.parent().unwrap());
}

#[test]
fn a_short_decode_in_a_non_final_extent_is_refused() {
    let img = image("short");
    let ino = mount(&img).lookup_path(FILE).unwrap().ino;
    // The FIRST extent, which is not the file's last.
    let mut first_extent = 0u64;
    let patched = edit_items(&img, ino, |key_type, key_offset, body| match key_type {
        // The checksums cover the bytes this edits, so the inode says not
        // to check them — as the write-side fixtures do.
        INODE_ITEM_KEY => {
            let flags = le64(body, INODE_FLAGS);
            body[INODE_FLAGS..INODE_FLAGS + 8]
                .copy_from_slice(&(flags | INODE_NODATACOW | INODE_NODATASUM).to_le_bytes());
            true
        }
        EXTENT_DATA_KEY if key_offset == 0 && body[COMPRESSION] != 0 => {
            first_extent = le64(body, DISK_BYTENR);
            false
        }
        _ => false,
    });
    assert!(patched >= 1, "fixture: the file's inode was found");
    assert_ne!(
        first_extent, 0,
        "fixture: the file's first extent was found"
    );

    // The stream's own length header, lowered to one segment: the decoder
    // stops there, cleanly, with a fraction of the bytes the item promises.
    {
        let physical = mount(&img)
            .chunk_map()
            .map(first_extent)
            .expect("the extent's address")
            .physical;
        let framing = Image::open(&img).read_at(physical, LZO_LEN + 4);
        let first_segment =
            u32::from_le_bytes(framing[LZO_LEN..LZO_LEN + 4].try_into().unwrap()) as usize;
        let shortened = (LZO_LEN + LZO_LEN + first_segment) as u32;
        write_window(&img, physical, &shortened.to_le_bytes());
    }

    match mount(&img).read_file(ino) {
        Err(e) => {
            let said = e.to_string();
            assert!(
                said.contains("decoded"),
                "refused, but not for the short decode: {said}"
            );
        }
        Ok(read) => {
            let zeros = read.iter().filter(|b| **b == 0).count();
            panic!(
                "read {} bytes through a stream that decodes short, {zeros} of them zeros",
                read.len()
            );
        }
    }
    let _ = std::fs::remove_dir_all(img.parent().unwrap());
}
