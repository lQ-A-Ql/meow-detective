//! A whole-file read of an inode claiming an impossible size fails with an
//! error, not an abort (#80).
//!
//! `read_file` bounded `inode.size` only by the volume's `total_bytes` and
//! then allocated it. On a large volume that is no bound at all. An
//! allocation that cannot be satisfied aborts the process, and
//! `capi::guard`'s `catch_unwind` cannot turn an abort into an errno. Here
//! the file's inode item claims 2^60 bytes and the superblock claims a
//! volume large enough to allow it.
//!
//! The image is `mkfs.btrfs --rootdir` of one small file, with the inode
//! item patched in every copy of its leaf and the primary superblock
//! patched, all restamped. `mkfs.btrfs` runs in the harness VM, which is
//! the one place the btrfs-progs tools live, so it is always there and
//! nothing here skips.

use fs_btrfs::btree::{header_offsets, HEADER_SIZE, ITEM_SIZE};
use fs_btrfs::chunk::objectid;
use fs_btrfs::fs::Filesystem;
use fs_btrfs::superblock::{offsets, Superblock};
use fs_btrfs_test_support::{le64, oracle, temp_path};
use fs_core::{BlockRead, FileDevice};
use std::sync::Arc;

const SUPERBLOCK: usize = 0x1_0000;
/// `btrfs_inode_item.size`.
const INODE_SIZE: usize = 16;
/// Removes the scratch directory on every exit path, a failed assertion
/// included.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// One small file, made into an image by `mkfs.btrfs --rootdir`.
///
/// The scratch tree lives inside this repository, because the guest that
/// runs `mkfs.btrfs` sees this repository and nothing else of the host:
/// an image under the host's `/tmp` is a path the tool cannot open.
fn image(tag: &str) -> (Scratch, std::path::PathBuf) {
    let dir = std::path::PathBuf::from(temp_path!("huge-size-{tag}"));
    let scratch = Scratch(dir.clone());
    let root = dir.join("root");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("file.bin"), b"a small file").unwrap();
    let img = dir.join("img");
    std::fs::File::create(&img)
        .unwrap()
        .set_len(256 * 1024 * 1024)
        .unwrap();
    let made = oracle("mkfs.btrfs")
        .args(["-f", "--rootdir"])
        .arg(&root)
        .arg(&img)
        .output();
    assert!(
        made.status.success(),
        "{}",
        String::from_utf8_lossy(&made.stderr)
    );
    (scratch, img)
}

/// 2^60 bytes, past any allocator; and one byte past the ceiling, well
/// inside the patched volume, which an overcommitting allocator would
/// have reserved and then failed to back.
#[test]
fn an_impossible_file_size_is_an_error_not_an_abort() {
    for claimed in [1u64 << 60, fs_btrfs::fs::MAX_WHOLE_FILE_READ + 1] {
        refuses(claimed);
    }
}

fn refuses(claimed: u64) {
    let (_scratch, img) = image(&claimed.to_string());
    let ino = {
        let fs = Filesystem::mount(Arc::new(FileDevice::open(&img).unwrap()) as Arc<dyn BlockRead>)
            .unwrap();
        let ino = fs.lookup_path("/file.bin").unwrap().ino;
        assert_eq!(
            fs.read_path("/file.bin").unwrap(),
            b"a small file",
            "control"
        );
        ino
    };

    let mut bytes = std::fs::read(&img).unwrap();
    let sb = Superblock::parse(&bytes[SUPERBLOCK..SUPERBLOCK + 4096]).unwrap();
    let node = sb.nodesize as usize;
    let mut patched = 0;
    for at in (0..bytes.len() - node).step_by(4096) {
        let block = &mut bytes[at..at + node];
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
            if le64(block, item) == ino && block[item + 8] == 1 {
                let off = HEADER_SIZE
                    + u32::from_le_bytes(block[item + 17..item + 21].try_into().unwrap()) as usize;
                block[off + INODE_SIZE..off + INODE_SIZE + 8]
                    .copy_from_slice(&claimed.to_le_bytes());
                hit = true;
            }
        }
        if hit {
            fs_btrfs::tree_write::stamp_checksum(block, &sb);
            patched += 1;
        }
    }
    assert!(patched >= 1, "fixture: the inode item was not found");
    let super_block = &mut bytes[SUPERBLOCK..SUPERBLOCK + 4096];
    super_block[offsets::TOTAL_BYTES..offsets::TOTAL_BYTES + 8]
        .copy_from_slice(&(1u64 << 62).to_le_bytes());
    fs_btrfs::super_write::stamp_checksum(super_block, sb.csum_type);
    std::fs::write(&img, &bytes).unwrap();

    let fs = Filesystem::mount(Arc::new(FileDevice::open(&img).unwrap()) as Arc<dyn BlockRead>)
        .expect("the patched volume still mounts");
    assert_eq!(
        fs.read_inode(ino).unwrap().size,
        claimed,
        "fixture: the size patch took"
    );
    match fs.read_file(ino) {
        Err(e @ fs_btrfs::Error::UnsupportedFeature(_)) => assert!(
            format!("{e}").contains("read_at"),
            "refused, but without saying what to use: {e}"
        ),
        Err(e) => panic!("refused as {e:?}, which reads as a device or volume fault"),
        Ok(_) => panic!("{claimed} bytes were read into memory"),
    }
}
