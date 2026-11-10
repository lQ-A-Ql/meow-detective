//! A relocation whose old block has no record where the plan expects one
//! is refused, not silently left recorded as allocated (#87).
//!
//! `apply_records` guarded `leaf_edit::delete` with "is the key there?",
//! so the editor's refusal for a missing record could never fire, and the
//! old block stayed recorded as allocated for ever. That is the ordinary
//! outcome on a volume without SKINNY_METADATA, where a tree block's record
//! is an EXTENT_ITEM under a different key.
//!
//! The image is a fresh `mkfs.btrfs`. The record of the root tree's root
//! block is re-keyed from METADATA_ITEM (169) to EXTENT_ITEM (168) -- the
//! non-skinny key type, which sorts in the same place -- in every copy of
//! its extent tree leaf, restamped. Relocating that block must then fail
//! to render. `mkfs.btrfs` runs in the harness VM, which is where this
//! suite's btrfs-progs lives, so the tool is never absent and nothing here
//! is conditional.

use fs_btrfs::btree::{header_offsets, HEADER_SIZE, ITEM_SIZE};
use fs_btrfs::chunk::{key_type, objectid};
use fs_btrfs::fs::Filesystem;
use fs_btrfs::superblock::Superblock;
use fs_btrfs::tree_write::stamp_checksum;
use fs_btrfs_test_support::{oracle, temp_path};
use fs_core::{BlockRead, FileDevice};
use std::sync::Arc;

const SUPERBLOCK: usize = 0x1_0000;

fn le64(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}

/// A fresh `mkfs.btrfs` image, under the suite's scratch directory inside
/// this repository -- the only tree the guest running the tool can see.
fn image() -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(temp_path!("extent-record"));
    std::fs::create_dir_all(&dir).unwrap();
    let img = dir.join("img");
    std::fs::File::create(&img)
        .unwrap()
        .set_len(256 * 1024 * 1024)
        .unwrap();
    let made = oracle("mkfs.btrfs").arg("-f").arg(&img).output();
    assert!(
        made.status.success(),
        "{}",
        String::from_utf8_lossy(&made.stderr)
    );
    img
}

fn mount(img: &std::path::Path) -> Filesystem {
    Filesystem::mount(Arc::new(FileDevice::open(img).unwrap()) as Arc<dyn BlockRead>).unwrap()
}

#[test]
fn a_block_whose_record_is_not_under_the_expected_key_is_not_relocated() {
    let img = image();
    let root = {
        let fs = mount(&img);
        let root = fs.superblock().root;
        // Control: the untouched volume renders the same relocation.
        let generation = fs.superblock().generation + 1;
        let plan = fs.plan_transaction_closed(&[root], 8).expect("planning");
        fs.render_plan(&plan, generation)
            .expect("control: the relocation renders on an untouched volume");
        root
    };

    let mut bytes = std::fs::read(&img).unwrap();
    let sb = Superblock::parse(&bytes[SUPERBLOCK..SUPERBLOCK + 4096]).unwrap();
    let node = sb.nodesize as usize;
    let mut rekeyed = 0;
    for at in (0..bytes.len() - node).step_by(4096) {
        let block = &mut bytes[at..at + node];
        if block[header_offsets::FSID..header_offsets::FSID + 16] != sb.fsid
            || le64(block, header_offsets::OWNER) != objectid::EXTENT_TREE
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
            if le64(block, item) == root && block[item + 8] == key_type::METADATA_ITEM {
                block[item + 8] = key_type::EXTENT_ITEM;
                hit = true;
            }
        }
        if hit {
            stamp_checksum(block, &sb);
            rekeyed += 1;
        }
    }
    assert!(
        rekeyed >= 1,
        "fixture: the root block's METADATA_ITEM was found"
    );
    std::fs::write(&img, &bytes).unwrap();

    let fs = mount(&img);
    let generation = fs.superblock().generation + 1;
    let plan = fs.plan_transaction_closed(&[root], 8).expect("planning");
    match fs.render_plan(&plan, generation) {
        Err(e) => assert!(
            format!("{e}").contains("nothing to remove"),
            "refused, but not for the missing record: {e}"
        ),
        Ok(_) => panic!("the relocation rendered, leaving the old block recorded as allocated"),
    }
    let _ = std::fs::remove_dir_all(img.parent().unwrap());
}
