//! Every leaf the kernel wrote, rebuilt and compared byte for byte.
//!
//! A copy-on-write writer's first act is to produce a tree block. If the
//! block is wrong the transaction cannot be right, and a block can be
//! wrong in ways that read back perfectly: an item offset measured from
//! the wrong place still parses if every item is measured the same way,
//! and a checksum over the wrong span is still a checksum.
//!
//! So the oracle is the kernel's own blocks. Every leaf in every fixture
//! is taken apart into its items, rebuilt through
//! [`fs_btrfs::tree_write::build_leaf`], and required to come back
//! identical — header, item array, item data and checksum.
//!
//! That is a stronger claim than a round trip through this crate's own
//! parser, which would pass even if both halves shared a misreading.
//!
//! The fixtures are gitignored and built by `chore fixtures`. A sweep
//! over "every fixture" that found none used to print a line and pass,
//! which is how this repository's leaf oracle ran against nothing at
//! all; there is now no list to be empty and no image to be skipped.

use fs_btrfs::btree::{header_offsets as o, TreeBlock, TreeGeometry, HEADER_SIZE};
use fs_btrfs::chunk::DiskKey;
use fs_btrfs::fs::Filesystem;
use fs_btrfs::superblock::Superblock;
use fs_btrfs::tree_write::{
    build_leaf, chunk_tree_uuid_of, stamp_checksum, BlockIdentity, LeafItem,
};
use fs_btrfs_test_support::{fixture, fixtures_matching, le32, le64, spans_several_devices, Image};
use fs_core::FileDevice;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Every fixture image, minus the members of a multi-device filesystem.
///
/// A pool member does not mount on its own, on purpose: its chunks may
/// live on the other disk. That is the one exclusion, and what is left
/// still has to be a list, or this sweep would re-encode nothing and
/// say so in the past tense.
fn images() -> Vec<PathBuf> {
    let out: Vec<PathBuf> = fixtures_matching("btrfs-")
        .into_iter()
        .filter(|p| !spans_several_devices(p))
        .collect();
    assert!(
        !out.is_empty(),
        "every fixture belongs to a multi-device filesystem, so there is no leaf here to \
         rebuild"
    );
    out
}

/// Take a leaf apart into the items it holds.
///
/// Deliberately does not use this crate's `Leaf` parser: the point is to
/// compare against the kernel's bytes, and reading them back through the
/// same code that will rebuild them would let a shared misunderstanding
/// pass.
fn items_of(block: &[u8]) -> Vec<(DiskKey, std::ops::Range<usize>)> {
    let nritems = le32(block, o::NRITEMS) as usize;
    let mut out = Vec::with_capacity(nritems);
    for i in 0..nritems {
        let at = HEADER_SIZE + i * 25;
        let key = DiskKey {
            objectid: le64(block, at),
            key_type: block[at + 8],
            offset: le64(block, at + 9),
        };
        // An item's offset is measured from the end of the header.
        let start = HEADER_SIZE + le32(block, at + 17) as usize;
        let size = le32(block, at + 21) as usize;
        out.push((key, start..start + size));
    }
    out
}

/// Every leaf reachable from a filesystem's trees.
///
/// Walking is done through the crate's own `Tree`, which is fine — what
/// is being validated is the *encoder*, and how the blocks were reached
/// does not affect whether the bytes match.
///
/// An image that will not open or will not mount is a failure here. It
/// used to be dropped from the sweep, and a driver that had stopped
/// mounting a whole geometry would have shown up as nothing worse than
/// a smaller count nobody reads.
fn leaves(img: &Path) -> (Superblock, Vec<Vec<u8>>) {
    let dev = Arc::new(
        FileDevice::open(img).unwrap_or_else(|error| panic!("opening {}: {error}", img.display())),
    );
    let fs = Filesystem::mount(dev)
        .unwrap_or_else(|error| panic!("mounting {}: {error}", img.display()));
    let sb = fs.superblock().clone();

    let mut blocks = Vec::new();

    // The blocks are found by scanning rather than by walking a tree,
    // and the checksum is what makes that safe: a run of file data
    // cannot fake a digest of itself. Scanning also reaches blocks no
    // walk would — older generations still on disk, and the trees this
    // crate has no walker for.
    //
    // Read directly: any block whose header says it is a
    // leaf of this filesystem is a candidate, and scanning is how they
    // are found without a second walker.
    //
    // A BLOCK AT A TIME, not the whole image: these fixtures are up to
    // 2 GiB and only the tree blocks are wanted. See `Image`.
    let image = Image::open(img);
    let nodesize = sb.nodesize as usize;
    let mut block = vec![0u8; nodesize];
    let mut at = 0u64;
    while image.try_read_at(at, &mut block) {
        let block = &block[..];
        at += nodesize as u64;

        // A leaf of this filesystem: right UUID, level zero, a plausible
        // item count, and a checksum that verifies. The checksum is what
        // makes the scan safe — a run of file data cannot fake it.
        if block[o::FSID..o::FSID + 16] != sb.fsid[..] || block[o::LEVEL] != 0 {
            continue;
        }
        let nritems = le32(block, o::NRITEMS) as usize;
        if nritems == 0 || HEADER_SIZE + nritems * 25 > nodesize {
            continue;
        }
        if !sb.csum_type.verify(&block[32..], &block[..32]) {
            continue;
        }
        blocks.push(block.to_vec());
    }
    (sb, blocks)
}

/// Rebuilding a leaf gives back exactly what the kernel wrote.
///
/// # Why the slack is copied rather than compared
///
/// A leaf's free space sits between the item array and the item data.
/// Neither side writes it: the kernel's block holds whatever was there
/// when the block was last used, a freshly built one holds zeros. The
/// checksum covers the whole block, so an honest comparison has to
/// decide what to do about those bytes.
///
/// This copies the kernel's slack into the rebuilt block and recomputes
/// the checksum. What is then asserted is: given the same slack, every
/// other byte AND the checksum are identical. That is the strong claim,
/// and it now applies to every block.
///
/// It did not before. The previous version compared the whole block only
/// when the slack happened to match — which meant the checksum was
/// checked on most blocks and, on the rest, replaced with "our checksum
/// covers our own bytes", a statement that is true of any self-
/// consistent block including a wrong one.
#[test]
fn every_leaf_re_encodes_identically() {
    let images = images();

    let mut total = 0usize;
    let mut with_dirty_slack = 0usize;

    for img in &images {
        let (sb, blocks) = leaves(img);
        let name = img.file_name().unwrap().to_string_lossy().into_owned();
        // Every btrfs filesystem has leaves, so an image with none is one
        // whose blocks this scan cannot recognise — a finding, not a
        // fixture to pass over.
        assert!(
            !blocks.is_empty(),
            "{name}: not one leaf of this filesystem was found on the disk"
        );

        for theirs in &blocks {
            let parsed = items_of(theirs);

            // An item pointing outside its own block was previously
            // skipped. It cannot be: the scan only accepts blocks whose
            // checksum verifies, so this is the kernel's own leaf and an
            // item that will not parse means `items_of` is reading the
            // item array wrongly.
            if let Some((key, range)) = parsed.iter().find(|(_, r)| r.end > theirs.len()) {
                panic!(
                    "{name}: the leaf at {} has an item for {key:?} spanning {range:?}, past \
                     the end of a {}-byte block. The block's checksum verified, so this is a \
                     misreading of the item array rather than a bad block.",
                    le64(theirs, o::BYTENR),
                    theirs.len()
                );
            }

            let items: Vec<LeafItem> = parsed
                .iter()
                .map(|(key, range)| LeafItem {
                    key: *key,
                    data: &theirs[range.clone()],
                })
                .collect();

            let id = BlockIdentity {
                bytenr: le64(theirs, o::BYTENR),
                owner: le64(theirs, o::OWNER),
                generation: le64(theirs, o::GENERATION),
                level: 0,
                flags: le64(theirs, o::FLAGS),
                chunk_tree_uuid: chunk_tree_uuid_of(theirs),
            };

            // A refusal used to be skipped. The kernel wrote this leaf,
            // so refusing it means the encoder has a rule the format
            // does not — which is a bug in exactly the direction that
            // silently shrinks what this test covers.
            let mut ours = build_leaf(&sb, id, &items).unwrap_or_else(|e| {
                panic!(
                    "{name}: the encoder refused the leaf at {} that the kernel wrote, with \
                     {} items: {e}",
                    id.bytenr,
                    items.len()
                )
            });

            let items_end = HEADER_SIZE + items.len() * 25;
            let data_start = theirs.len() - items.iter().map(|i| i.data.len()).sum::<usize>();

            if ours[items_end..data_start] != theirs[items_end..data_start] {
                with_dirty_slack += 1;
            }
            ours[items_end..data_start].copy_from_slice(&theirs[items_end..data_start]);
            // The checksum covers the slack, so it has to be recomputed
            // after the copy or it would be a digest of the wrong block.
            stamp_checksum(&mut ours, &sb);

            if let Some(i) = (0..theirs.len()).find(|&i| ours[i] != theirs[i]) {
                panic!(
                    "{name}: the leaf at {} differs at byte {i:#x} — {} (ours {:#04x}, \
                     kernel {:#04x}). {} items, array ends at {items_end:#x}, data starts \
                     at {data_start:#x}.",
                    id.bytenr,
                    where_in_leaf(i, items_end, data_start, items.len()),
                    ours[i],
                    theirs[i],
                    items.len()
                );
            }

            total += 1;
        }
    }

    assert!(
        total > 20,
        "only {total} leaves were rebuilt across {} images, which is too few to have \
         exercised the encoder",
        images.len()
    );
    eprintln!(
        "{total} kernel leaves rebuilt byte for byte across {} images, checksum included; \
         {with_dirty_slack} of them had slack the kernel never cleared",
        images.len()
    );
}

/// Name the part of a leaf a byte offset falls in.
///
/// A bare offset makes a failure hard to read: 0x76 could be the header,
/// an item's key, or its length field, and which one it is names the
/// bug.
fn where_in_leaf(at: usize, items_end: usize, data_start: usize, nritems: usize) -> String {
    if at < 32 {
        return "the checksum".to_string();
    }
    if at < HEADER_SIZE {
        let field = match at {
            0x20..=0x2f => "fsid",
            0x30..=0x37 => "bytenr",
            0x38..=0x3f => "flags",
            0x40..=0x4f => "chunk_tree_uuid",
            0x50..=0x57 => "generation",
            0x58..=0x5f => "owner",
            0x60..=0x63 => "nritems",
            _ => "level",
        };
        return format!("the header's {field}");
    }
    if at < items_end {
        let i = (at - HEADER_SIZE) / 25;
        let within = (at - HEADER_SIZE) % 25;
        let field = match within {
            0..=16 => "key",
            17..=20 => "OFFSET — measured from the end of the header, not the block start",
            _ => "size",
        };
        return format!("item {i} of {nritems}, its {field}");
    }
    if at < data_start {
        return "the free space between the item array and the item data".to_string();
    }
    "the item data".to_string()
}

/// The encoder builds leaves it has not seen, not only ones it is
/// copying.
///
/// Re-encoding proves the layout is reproduced. It cannot prove the
/// layout is COMPUTED, because every input comes from a block that
/// already has the right answer in it — an offset bug that happens to
/// cancel for the item counts on disk would pass. A writer never
/// reproduces a leaf; it builds one whose contents are new.
///
/// So: take a kernel leaf, drop an item from the middle, rebuild, and
/// read the result back with this crate's own parser. Every remaining
/// item must be there, in order, with its data intact — at offsets
/// nothing on disk has ever held.
#[test]
fn a_leaf_built_with_an_item_removed_reads_back_correctly() {
    let images = images();

    let mut checked = 0usize;
    for img in &images {
        let (sb, blocks) = leaves(img);
        let name = img.file_name().unwrap().to_string_lossy().into_owned();
        let geom = TreeGeometry::from_superblock(&sb);

        for theirs in blocks.iter().take(40) {
            let parsed = items_of(theirs);
            if parsed.len() < 3 || parsed.iter().any(|(_, r)| r.end > theirs.len()) {
                continue;
            }

            let drop_at = parsed.len() / 2;
            let expected: Vec<(DiskKey, Vec<u8>)> = parsed
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != drop_at)
                .map(|(_, (k, r))| (*k, theirs[r.clone()].to_vec()))
                .collect();

            let items: Vec<LeafItem> = expected
                .iter()
                .map(|(key, data)| LeafItem {
                    key: *key,
                    data: data.as_slice(),
                })
                .collect();

            let bytenr = le64(theirs, o::BYTENR);
            let built = build_leaf(
                &sb,
                BlockIdentity {
                    bytenr,
                    owner: le64(theirs, o::OWNER),
                    generation: le64(theirs, o::GENERATION),
                    level: 0,
                    flags: le64(theirs, o::FLAGS),
                    chunk_tree_uuid: chunk_tree_uuid_of(theirs),
                },
                &items,
            )
            .unwrap_or_else(|e| panic!("{name}: rebuilding {bytenr} with one item fewer: {e}"));

            // Through the crate's reader, which is the consumer that
            // matters: a block only this test can read is not a block.
            let block = TreeBlock::parse(built, bytenr, &geom).unwrap_or_else(|e| {
                panic!("{name}: the leaf built at {bytenr} will not parse: {e}")
            });

            let read = block
                .body
                .items()
                .unwrap_or_else(|| panic!("{name}: the block built at {bytenr} is not a leaf"));

            assert_eq!(
                read.len(),
                expected.len(),
                "{name}: built {} items into {bytenr} and read {} back",
                expected.len(),
                read.len()
            );

            for (i, item) in read.iter().enumerate() {
                let (key, data): &(DiskKey, Vec<u8>) = &expected[i];
                assert_eq!(
                    (item.key.objectid, item.key.key_type, item.key.offset),
                    (key.objectid, key.key_type, key.offset),
                    "{name}: item {i} of the leaf built at {bytenr} came back under the \
                     wrong key"
                );
                let got = block.item_data(item).unwrap_or_else(|| {
                    panic!(
                        "{name}: item {i} of the leaf built at {bytenr} points outside the \
                         block — its offset is wrong for this item count"
                    )
                });
                assert_eq!(
                    got,
                    data.as_slice(),
                    "{name}: item {i} of the leaf built at {bytenr} came back with the \
                     wrong bytes, so its offset or size is wrong"
                );
            }

            checked += 1;
            if checked >= 200 {
                break;
            }
        }
        if checked >= 200 {
            break;
        }
    }

    assert!(
        checked > 0,
        "no fixture had a leaf with three items to rebuild, so the encoder was never \
         asked to compute a layout it had not been handed"
    );
    eprintln!("{checked} leaves rebuilt with an item removed and read back correctly");
}

/// A leaf that will not fit is refused rather than truncated.
#[test]
fn an_overfull_leaf_is_refused() {
    let (sb, _) = leaves(&fixture("btrfs-default.img"));

    let big = vec![0u8; sb.nodesize as usize];
    let items = [LeafItem {
        key: DiskKey {
            objectid: 1,
            key_type: 1,
            offset: 0,
        },
        data: &big,
    }];
    let err = build_leaf(
        &sb,
        BlockIdentity {
            bytenr: 0,
            owner: 5,
            generation: 1,
            level: 0,
            flags: 0,
            chunk_tree_uuid: [0; 16],
        },
        &items,
    )
    .expect_err("an item the size of the whole block cannot fit alongside a header");
    // The condition and the way out, not the wording. A third
    // assertion on the exact sentence is how the old message went on
    // saying splitting was unimplemented while `leaf_edit::split` sat
    // beside it — and this one only runs under the kernel-validation
    // job, so it was the last to say so.
    assert!(
        err.to_string().contains("a tree block holds"),
        "the refusal should name what went wrong: {err}"
    );
    assert!(
        err.to_string().contains("leaf_edit::split"),
        "the refusal should name what the answer would be: {err}"
    );
}

/// A leaf carried to a different address is rejected when read there.
///
/// This is the invariant the encoder's own docs name — "a block records
/// its own address ... which is the behaviour that catches a
/// copy-on-write writer that forgot the block moved" — and nothing
/// tested it. It is the characteristic copy-on-write bug: the whole
/// point of the write path is that blocks move, so a writer that copies
/// a block without re-stamping produces one that parses, checksums
/// correctly, and is wrong.
#[test]
fn a_leaf_read_at_an_address_it_was_not_built_for_is_rejected() {
    let (sb, blocks) = leaves(&fixture("btrfs-default.img"));
    let theirs = blocks
        .first()
        .expect("btrfs-default.img holds at least one leaf");

    let geom = TreeGeometry::from_superblock(&sb);
    let parsed = items_of(theirs);
    // The scan only accepts blocks whose checksum verifies, so an item
    // reaching past the end of one is a misreading of the item array
    // rather than a leaf to leave alone.
    assert!(
        !parsed.iter().any(|(_, r)| r.end > theirs.len()),
        "an item of the leaf at {} spans past the end of the block",
        le64(theirs, o::BYTENR)
    );
    let items: Vec<LeafItem> = parsed
        .iter()
        .map(|(key, range)| LeafItem {
            key: *key,
            data: &theirs[range.clone()],
        })
        .collect();

    let built_for = le64(theirs, o::BYTENR);
    let block = build_leaf(
        &sb,
        BlockIdentity {
            bytenr: built_for,
            owner: le64(theirs, o::OWNER),
            generation: le64(theirs, o::GENERATION),
            level: 0,
            flags: le64(theirs, o::FLAGS),
            chunk_tree_uuid: chunk_tree_uuid_of(theirs),
        },
        &items,
    )
    .expect("rebuilding a leaf the kernel wrote");

    // At the address it was built for, it reads.
    TreeBlock::parse(block.clone(), built_for, &geom)
        .expect("a block read at its own address must parse");

    // One node further on, it must not — the bytes are a perfectly valid
    // block, and the only thing wrong is where it is.
    let elsewhere = built_for + sb.nodesize as u64;
    assert!(
        TreeBlock::parse(block, elsewhere, &geom).is_err(),
        "a leaf stamped for {built_for} was accepted when read at {elsewhere}. Nothing then          catches a copy-on-write writer that moves a block without re-stamping it."
    );
}

/// A leaf claiming a level above zero is refused.
///
/// `build_node` already refuses the mirror of this. Without it the level
/// was the one identity field nothing checked, and a mutation that
/// stopped writing it survived every test here — a leaf's level is zero
/// and an unwritten byte is zero, so nothing about a LEAF can tell the
/// difference. What can be checked is that a caller passing a level a
/// leaf cannot have is told so.
#[test]
fn a_leaf_that_claims_to_be_a_node_is_refused() {
    let (sb, _) = leaves(&fixture("btrfs-default.img"));

    let data = [0u8; 8];
    let items = [LeafItem {
        key: DiskKey {
            objectid: 1,
            key_type: 1,
            offset: 0,
        },
        data: &data,
    }];
    let id = BlockIdentity {
        bytenr: 0,
        owner: 5,
        generation: 1,
        level: 1,
        flags: 0,
        chunk_tree_uuid: [0; 16],
    };

    let err = build_leaf(&sb, id, &items).expect_err("level 1 with items is a contradiction");
    assert!(
        err.to_string().contains("level 0"),
        "the refusal should name the contradiction: {err}"
    );

    // And level 0 is accepted, so this is not a check that refuses
    // everything.
    assert!(build_leaf(&sb, BlockIdentity { level: 0, ..id }, &items).is_ok());
}

/// Items out of order are refused, because a search bisects on that
/// order and an unsorted leaf finds some items and silently misses
/// others.
#[test]
fn unsorted_items_are_refused() {
    let (sb, _) = leaves(&fixture("btrfs-default.img"));

    let a = [1u8; 8];
    let items = [
        LeafItem {
            key: DiskKey {
                objectid: 9,
                key_type: 1,
                offset: 0,
            },
            data: &a,
        },
        LeafItem {
            key: DiskKey {
                objectid: 2,
                key_type: 1,
                offset: 0,
            },
            data: &a,
        },
    ];
    assert!(
        build_leaf(
            &sb,
            BlockIdentity {
                bytenr: 0,
                owner: 5,
                generation: 1,
                level: 0,
                flags: 0,
                chunk_tree_uuid: [0; 16],
            },
            &items,
        )
        .is_err(),
        "an unsorted leaf must be refused"
    );
}
