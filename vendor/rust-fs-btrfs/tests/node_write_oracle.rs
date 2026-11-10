//! Every internal node the kernel wrote, rebuilt and compared.
//!
//! Copy-on-write means a change to one leaf rewrites the whole spine
//! above it: a new leaf, a new node pointing at it, a new node above
//! that, up to the root. So the nodes are half of what a transaction
//! produces, and a node can be wrong in a way that reads back perfectly
//! — every pointer parses, the descent terminates, and a search returns
//! "not found" for a key that is there.
//!
//! The same oracle as the leaves: take the kernel's own nodes apart,
//! rebuild them through [`fs_btrfs::tree_write::build_node`], and
//! require the bytes back.
//!
//! The fixtures are gitignored and built by `chore fixtures`, in the
//! fs-linux-test-harness VM — the nodes rebuilt here are that kernel's.
//! A missing fixture fails the test that wanted it rather than emptying
//! it: the leaf oracle beside this one spent a release returning early
//! on fixtures it never found, and reported green throughout.

use fs_btrfs::btree::{header_offsets as o, KeyPtr, HEADER_SIZE, ITEM_SIZE, KEY_PTR_SIZE};
use fs_btrfs::chunk::DiskKey;
use fs_btrfs::fs::Filesystem;
use fs_btrfs::superblock::Superblock;
use fs_btrfs::tree_write::{build_node, chunk_tree_uuid_of, key_ptr_capacity, BlockIdentity};
use fs_btrfs_test_support::{fixture, fixtures_matching, le32, le64, spans_several_devices, Image};
use fs_core::FileDevice;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// `BTRFS_ROOT_TREE_OBJECTID`, the owner every root-tree block records.
const ROOT_TREE: u64 = 1;
/// `BTRFS_ROOT_ITEM_KEY`. Spelled out here rather than added to a module
/// this test is not allowed to depend on, as `tests/btree_oracle.rs`
/// does with the same constant.
const ROOT_ITEM_KEY: u8 = 132;
/// `bytenr` within a `btrfs_root_item`: the 160-byte inode item, then
/// `generation`, then the root's block address.
const ROOT_ITEM_BYTENR: usize = 176;

/// Every fixture whose nodes can be scanned out of the image.
fn images() -> Vec<PathBuf> {
    let images: Vec<PathBuf> = fixtures_matching("btrfs-")
        .into_iter()
        // One member of a multi-device filesystem is refused a mount on
        // purpose, and its filesystem's blocks are not all on this
        // disk. `tests/pool_oracle.rs` is where such an image belongs.
        .filter(|p| !spans_several_devices(p))
        .collect();
    assert!(
        !images.is_empty(),
        "every fixture belongs to a multi-device filesystem, so there is no image here \
         to scan for nodes"
    );
    images
}

/// Take a node apart into its key pointers.
///
/// Read here rather than through the crate's `KeyPtr::parse` for the
/// same reason the leaf oracle re-implements its own reader: a shared
/// misunderstanding of the packed layout would cancel out and pass.
fn ptrs_of(block: &[u8]) -> Vec<KeyPtr> {
    let nritems = le32(block, o::NRITEMS) as usize;
    (0..nritems)
        .map(|i| {
            let at = HEADER_SIZE + i * KEY_PTR_SIZE;
            KeyPtr {
                key: DiskKey {
                    objectid: le64(block, at),
                    key_type: block[at + 8],
                    offset: le64(block, at + 9),
                },
                // Packed: both u64s start on odd offsets, 17 and 25.
                blockptr: le64(block, at + 17),
                generation: le64(block, at + 25),
            }
        })
        .collect()
}

/// Every internal node on a filesystem, found by scanning.
///
/// Scanning rather than walking, so it reaches nodes no walk from the
/// current root would — older generations still on disk, and the trees
/// this crate has no walker for. The checksum is what makes that safe:
/// a run of file data cannot fake a digest of itself.
fn nodes(img: &Path) -> (Superblock, Vec<Vec<u8>>) {
    let sb = superblock(img);

    // A BLOCK AT A TIME, not the whole image: these fixtures are up to
    // 2 GiB and only the tree blocks are wanted. See `Image`.
    let image = Image::open(img);
    let nodesize = sb.nodesize as usize;
    let mut blocks = Vec::new();
    let mut buf = vec![0u8; nodesize];
    let mut at = 0u64;
    while image.try_read_at(at, &mut buf) {
        let block = &buf[..];
        at += nodesize as u64;

        // A node of this filesystem: right UUID, level above zero, an
        // item count that fits, and a checksum that verifies.
        if block[o::FSID..o::FSID + 16] != sb.fsid[..] || block[o::LEVEL] == 0 {
            continue;
        }
        let nritems = le32(block, o::NRITEMS) as usize;
        if nritems == 0 || HEADER_SIZE + nritems * KEY_PTR_SIZE > nodesize {
            continue;
        }
        if !sb.csum_type.verify(&block[32..], &block[..32]) {
            continue;
        }
        blocks.push(block.to_vec());
    }
    (sb, blocks)
}

/// A fixture's superblock, read the way a mount reads it.
///
/// The refusal tests below need one real superblock and no nodes at
/// all, and scanning a 400 MiB image for blocks they will not look at
/// is work for nothing.
fn superblock(img: &Path) -> Superblock {
    let dev = Arc::new(
        FileDevice::open(img).unwrap_or_else(|e| panic!("opening {}: {e}", img.display())),
    );
    Filesystem::mount(dev)
        .unwrap_or_else(|e| panic!("mounting {}: {e}", img.display()))
        .superblock()
        .clone()
}

/// Rebuilding a node gives back exactly what the kernel wrote.
#[test]
fn every_node_re_encodes_identically() {
    let mut total = 0usize;
    let mut exact = 0usize;
    let mut images_with_nodes = 0usize;
    let mut deepest = 0u8;

    let images = images();
    for img in &images {
        let (sb, blocks) = nodes(img);
        if blocks.is_empty() {
            continue;
        }
        images_with_nodes += 1;
        let name = img.file_name().unwrap().to_string_lossy().into_owned();

        for theirs in &blocks {
            let ptrs = ptrs_of(theirs);
            let level = theirs[o::LEVEL];
            deepest = deepest.max(level);

            let id = BlockIdentity {
                bytenr: le64(theirs, o::BYTENR),
                owner: le64(theirs, o::OWNER),
                generation: le64(theirs, o::GENERATION),
                level,
                flags: le64(theirs, o::FLAGS),
                chunk_tree_uuid: chunk_tree_uuid_of(theirs),
            };

            let ours = match build_node(&sb, id, &ptrs) {
                Ok(b) => b,
                Err(e) => panic!(
                    "{name}: node at {} was refused: {e}. The kernel wrote it, so \
                     refusing it names a rule this encoder has that the format does not.",
                    id.bytenr
                ),
            };

            // Header and pointer array must match exactly. The slack
            // after them holds whatever the block last held, so the
            // checksum can only match when the slack does — the same
            // three-part comparison the leaf oracle makes, for the same
            // reason.
            let ptrs_end = HEADER_SIZE + ptrs.len() * KEY_PTR_SIZE;
            // Named byte by byte rather than by comparing the slices
            // directly: a failed `assert_eq!` on two 16 KiB blocks
            // prints both of them, and the one number that identifies
            // the bug is the offset.
            if let Some(i) = (32..ptrs_end).find(|&i| ours[i] != theirs[i]) {
                let (what, within) = if i < HEADER_SIZE {
                    ("the header", i)
                } else {
                    ("key pointer", (i - HEADER_SIZE) % KEY_PTR_SIZE)
                };
                panic!(
                    "{name}: node at {} differs at byte {i:#x} — {what}, offset {within} \
                     into it (ours {:#04x}, kernel {:#04x}). A key pointer is a 17-byte \
                     key, then blockptr at 17 and generation at 25.",
                    id.bytenr, ours[i], theirs[i]
                );
            }

            if ours[ptrs_end..] == theirs[ptrs_end..] {
                assert_eq!(
                    ours, *theirs,
                    "{name}: node at {} has identical content and slack, so every byte \
                     including the checksum should match",
                    id.bytenr
                );
                exact += 1;
            } else {
                assert!(
                    sb.csum_type.verify(&ours[32..], &ours[..32]),
                    "{name}: node at {} carries a checksum that does not cover it",
                    id.bytenr
                );
            }
            total += 1;
        }
    }

    // Fixtures exist but none had a node: that is a regression in the
    // fixture matrix, not a reason to pass. A skip here reads exactly
    // like success, which is how the leaf oracle went a release running
    // against nothing.
    assert!(
        images_with_nodes > 0,
        "{} fixtures were read and not one had a tree above level 0, so this test \
         exercised nothing. The deep fixtures — the `populated` target of \
         `chore fixtures` — are what produce nodes.",
        images.len()
    );
    eprintln!(
        "{total} kernel nodes rebuilt across {images_with_nodes} images, deepest level \
         {deepest}; {exact} matched every byte including the checksum"
    );
    assert!(
        exact > 0,
        "not one node matched byte for byte — if the slack always differs, the comparison \
         never reaches the checksum"
    );
}

/// Every block of this filesystem on the image, indexed by the LOGICAL
/// address it records, keeping the newest copy of each.
///
/// Indexed by the block's own `bytenr` field, so a child is found
/// without resolving logical addresses through the chunk tree — which is
/// what keeps the check below independent of the reader it is judging.
///
/// NEWEST WINS, and the two words are doing different jobs. A DUP
/// profile writes two physical copies of every metadata block at one
/// logical address; they are identical, so either will do. An address
/// freed in one transaction and handed out again in a later one leaves
/// the older block on disk as well; the live one is the later
/// generation, and the stale one is not part of any tree.
fn blocks_by_bytenr(image: &Image, sb: &Superblock) -> HashMap<u64, Vec<u8>> {
    let nodesize = sb.nodesize as usize;
    let mut index: HashMap<u64, Vec<u8>> = HashMap::new();
    let mut buf = vec![0u8; nodesize];
    let mut at = 0u64;
    while image.try_read_at(at, &mut buf) {
        let block = &buf[..];
        at += nodesize as u64;
        if block[o::FSID..o::FSID + 16] != sb.fsid[..] {
            continue;
        }
        let nritems = le32(block, o::NRITEMS) as usize;
        if nritems == 0 || HEADER_SIZE + nritems * KEY_PTR_SIZE > nodesize {
            continue;
        }
        if !sb.csum_type.verify(&block[32..], &block[..32]) {
            continue;
        }
        let bytenr = le64(block, o::BYTENR);
        let generation = le64(block, o::GENERATION);
        match index.get(&bytenr) {
            Some(seen) if le64(seen, o::GENERATION) >= generation => {}
            _ => {
                index.insert(bytenr, block.to_vec());
            }
        }
    }
    index
}

/// The nodes the superblock can still REACH, walked from its roots.
///
/// WHY REACHABILITY AND NOT A SCAN, WHICH IS WHAT THE TEST ABOVE USES.
/// The two tests ask different questions of the same image. Re-encoding
/// a node is a property of that node alone, so the more nodes the
/// better and every block the kernel ever wrote is fair game. The
/// descent invariant is a property of a node AND its children TOGETHER,
/// and it only holds for a node that is still part of a tree.
///
/// A superseded node breaks it without anything being wrong. Inside one
/// transaction the kernel writes dirty tree blocks out as memory
/// pressure demands and keeps going: a leaf already written at address
/// B is modified again in place, its parent is copied to a new address
/// with the corrected key, and the parent's earlier copy stays on disk
/// naming B with the key B held when that copy was written. Both
/// parents then carry the same generation, both verify their checksum,
/// and only the reachable one is the tree's.
///
/// That is not a corner case, and it is not rare. The populated
/// fixtures write sixty thousand files in a single transaction, and on
/// `btrfs-deep16k` a scan finds 120 nodes of which SEVEN are reachable:
/// the other 113 are superseded copies from the same transaction. It
/// failed on every machine that built those fixtures, with a node at
/// logical 76185600 naming a leaf whose first key had moved on by the
/// time the transaction committed.
///
/// The walk starts at the superblock's own roots and follows the root
/// tree's `ROOT_ITEM`s to every other tree, so it needs no chunk-tree
/// resolution and stays as independent of the crate as the scan is.
fn live_nodes(index: &HashMap<u64, Vec<u8>>, sb: &Superblock) -> Vec<Vec<u8>> {
    let mut queue: VecDeque<u64> = [sb.root, sb.chunk_root, sb.log_root]
        .into_iter()
        .filter(|&b| b != 0)
        .collect();
    let mut seen: HashSet<u64> = queue.iter().copied().collect();
    let mut nodes = Vec::new();

    while let Some(bytenr) = queue.pop_front() {
        let Some(block) = index.get(&bytenr) else {
            // A root recorded in a tree whose block is not on this image
            // — a log tree the mount replayed away, say. Not reachable,
            // so not this test's business.
            continue;
        };
        let nritems = le32(block, o::NRITEMS) as usize;
        if block[o::LEVEL] > 0 {
            for ptr in ptrs_of(block) {
                if seen.insert(ptr.blockptr) {
                    queue.push_back(ptr.blockptr);
                }
            }
            nodes.push(block.clone());
            continue;
        }
        // A root-tree leaf names the root of every other tree. Read by
        // hand, like everything else here.
        if le64(block, o::OWNER) != ROOT_TREE {
            continue;
        }
        for i in 0..nritems {
            let item = HEADER_SIZE + i * ITEM_SIZE;
            if item + ITEM_SIZE > block.len() || block[item + 8] != ROOT_ITEM_KEY {
                continue;
            }
            let at = HEADER_SIZE + le32(block, item + 17) as usize;
            let size = le32(block, item + 21) as usize;
            if at + size > block.len() || size < ROOT_ITEM_BYTENR + 8 {
                continue;
            }
            let root = le64(block, at + ROOT_ITEM_BYTENR);
            if root != 0 && seen.insert(root) {
                queue.push_back(root);
            }
        }
    }
    nodes
}

/// The keys in a kernel node really are the smallest key of the child.
///
/// This is the rule a node encoder can break without breaking anything
/// that parses: descent takes the last child whose key is <= the one
/// sought, so a key that is too large skips a subtree silently. Checked
/// against the kernel's own trees rather than asserted in prose.
#[test]
fn each_pointer_key_is_the_first_key_of_the_child_it_names() {
    let mut checked = 0usize;
    let mut walked = 0usize;
    let images = images();
    for img in &images {
        let sb = superblock(img);
        let index = blocks_by_bytenr(&Image::open(img), &sb);
        let nodes = live_nodes(&index, &sb);
        walked += nodes.len();

        for node in &nodes {
            for ptr in ptrs_of(node) {
                let child = index.get(&ptr.blockptr).unwrap_or_else(|| {
                    panic!(
                        "{}: a live node names a child at {} that is not on this image, \
                         so the tree the superblock reaches is incomplete",
                        img.display(),
                        ptr.blockptr
                    )
                });
                // The child's first key, whether it is a leaf or a node:
                // both start their array at the same offset and both
                // open it with a 17-byte key.
                let first = DiskKey {
                    objectid: le64(child, HEADER_SIZE),
                    key_type: child[HEADER_SIZE + 8],
                    offset: le64(child, HEADER_SIZE + 9),
                };
                assert_eq!(
                    (ptr.key.objectid, ptr.key.key_type, ptr.key.offset),
                    (first.objectid, first.key_type, first.offset),
                    "{}: a pointer to {} carries {:?} but the child's first key is {:?}",
                    img.display(),
                    ptr.blockptr,
                    ptr.key,
                    first
                );
                checked += 1;
            }
        }
    }

    assert!(
        checked > 0,
        "not one parent/child pair was reachable across {} fixtures, so the descent \
         invariant was never checked",
        images.len()
    );
    eprintln!(
        "{checked} pointer keys are the first key of the child they name, across {walked} \
         live nodes"
    );
}

/// A node at level 0 is refused: that is a leaf, and a block claiming
/// both is one a reader walks off the bottom of.
#[test]
fn a_node_cannot_claim_to_be_a_leaf() {
    // Any fixture would do — this needs a superblock, not a node — so
    // it names the plainest one.
    let sb = superblock(&fixture("btrfs-default.img"));

    let ptr = KeyPtr {
        key: DiskKey {
            objectid: 1,
            key_type: 1,
            offset: 0,
        },
        blockptr: 4096,
        generation: 1,
    };
    let err = build_node(
        &sb,
        BlockIdentity {
            bytenr: 0,
            owner: 1,
            generation: 1,
            level: 0,
            flags: 0,
            chunk_tree_uuid: [0; 16],
        },
        &[ptr],
    )
    .expect_err("level 0 with key pointers is a contradiction");
    assert!(
        err.to_string().contains("level 0"),
        "the refusal should name the contradiction: {err}"
    );
}

/// More pointers than fit are refused rather than truncated.
#[test]
fn an_overfull_node_is_refused() {
    let sb = superblock(&fixture("btrfs-default.img"));

    let capacity = key_ptr_capacity(&sb);
    let ptrs: Vec<KeyPtr> = (0..capacity as u64 + 1)
        .map(|i| KeyPtr {
            key: DiskKey {
                objectid: i,
                key_type: 1,
                offset: 0,
            },
            blockptr: 4096 * (i + 1),
            generation: 1,
        })
        .collect();

    assert!(
        build_node(
            &sb,
            BlockIdentity {
                bytenr: 0,
                owner: 1,
                generation: 1,
                level: 1,
                flags: 0,
                chunk_tree_uuid: [0; 16],
            },
            &ptrs,
        )
        .is_err(),
        "one more than capacity ({capacity}) must be refused"
    );

    // And exactly capacity must be accepted, or the bound is off by one
    // in the direction that wastes a slot on every node.
    assert!(
        build_node(
            &sb,
            BlockIdentity {
                bytenr: 0,
                owner: 1,
                generation: 1,
                level: 1,
                flags: 0,
                chunk_tree_uuid: [0; 16],
            },
            &ptrs[..capacity],
        )
        .is_ok(),
        "exactly capacity ({capacity}) must fit"
    );
}
