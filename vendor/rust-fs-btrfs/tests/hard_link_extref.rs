//! Hard links, read across the `INODE_REF` to `INODE_EXTREF` spill (#196).
//!
//! # What the spill is
//!
//! A btrfs inode names each of its links in a back-reference: one
//! `INODE_REF` item per parent directory, keyed `(ino, INODE_REF, parent)`,
//! packing `index, namelen, name` for every link that inode has in that
//! directory. That item can only grow as far as one leaf allows. The kernel
//! extends it in `btrfs_insert_inode_ref`, and once extending it would
//! leave an item larger than the leaf `split_leaf` answers `-EOVERFLOW`;
//! with the `extref` feature that link goes into an `INODE_EXTREF` item
//! instead, keyed `(ino, INODE_EXTREF, hash(parent, name))`. Where that
//! happens depends on the node size AND on the length of the names, which
//! is why a file with two links, or ten short ones, never reaches it.
//!
//! # What is checked, and against what
//!
//! Nothing here is ours on the expected side. For three geometries — 4 KiB
//! nodes with 255-byte names, 4 KiB with 64-byte names, 16 KiB with
//! 255-byte names — `mkfs.btrfs` makes an empty volume and THE KERNEL, in
//! the harness guest, creates the links: up to exactly the last name the
//! `INODE_REF` holds, one past it, three past it, then unlinks one of the
//! names in the `INODE_REF` and finally every name that had spilled, so the
//! count crosses the boundary downwards as well as up. After every step:
//!
//! - `btrfs check` must call the volume clean;
//! - `btrfs inspect-internal dump-tree` must show exactly which names sit in
//!   the `INODE_REF` and which in `INODE_EXTREF` items, and the spilled
//!   names must be the ones predicted. That is what stops this passing
//!   while missing the boundary: a sweep that never produced an
//!   `INODE_EXTREF` fails here, not in a comment;
//! - and this crate must agree with the kernel's own `stat`: the same link
//!   count, every name listed, every path resolving to the same inode, and
//!   the same bytes read through each of them.
//!
//! This crate cannot create or remove a link (its only write path is
//! in-place `nodatacow`, #61), so the kernel does the writing and the
//! direction checked is the read.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use fs_btrfs::Filesystem;
use fs_btrfs_test_support::{
    assert_btrfs_check_clean, dump_tree, guest_kernel_write_ok, oracle, sha256_hex, temp_path,
};
use fs_core::FileDevice;

/// `sizeof(struct btrfs_header)`: a leaf's data area is the node less this.
const HEADER: usize = 101;
/// `sizeof(struct btrfs_item)`: the per-item header in a leaf.
const ITEM: usize = 25;
/// `sizeof(struct btrfs_inode_ref)`: `index` (8) and `namelen` (2).
const REF_ENTRY: usize = 10;

/// The directory holding the links under test, and a second one holding
/// one more link, so the count also spans a second `INODE_REF` item.
const DIR: &str = "d";
const OTHER: &str = "other/far";

/// How many names of `name_len` bytes the one `INODE_REF` item holds on a
/// volume with `node_size` nodes before the next link spills.
///
/// `btrfs_insert_inode_ref` extends the existing item; `split_leaf` refuses
/// (`-EOVERFLOW`) when `data_size + item_size + sizeof(btrfs_item)` exceeds
/// the leaf's data area, where `data_size` is the new entry plus its item
/// header. So the `k`-th name fits while `k * entry <= leaf - 2 * ITEM`.
fn names_that_fit(node_size: usize, name_len: usize) -> usize {
    (node_size - HEADER - 2 * ITEM) / (REF_ENTRY + name_len)
}

/// Link `i`'s name: the index, zero-padded, then padding to `len` bytes.
/// Mirrored by `nm` in [`GUEST_PRELUDE`].
fn name(i: usize, len: usize) -> String {
    format!("{i:05}{}", "n".repeat(len - 5))
}

/// Shell the guest runs before every step: `nm` builds the same names as
/// [`name`], and the step's own commands follow it.
const GUEST_PRELUDE: &str = r#"
cd "$MNT"
nm() { printf '%05d' "$1"; printf '%*s' $((LEN - 5)) '' | tr ' ' n; }
"#;

/// Shell the guest runs after every step: what the kernel says the links
/// are, as `kind<TAB>...` lines.
const GUEST_REPORT: &str = r#"
sync
printf 'dir\t%s\n' "$(stat -c %i d)"
printf 'sha\t%s\n' "$(sha256sum other/far | cut -d' ' -f1)"
for path in d/* other/*; do
    printf 'link\t%s\t%s\t%s\n' "$path" "$(stat -c %i "$path")" "$(stat -c %h "$path")"
done
"#;

/// What the kernel reported after one step.
struct KernelView {
    dir_ino: u64,
    sha: String,
    /// `(path, ino, nlink)` for every link, in both directories.
    links: Vec<(String, u64, u32)>,
}

fn parse_kernel(out: &str) -> KernelView {
    let mut view = KernelView {
        dir_ino: 0,
        sha: String::new(),
        links: Vec::new(),
    };
    for line in out.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        match fields.as_slice() {
            ["dir", ino] => view.dir_ino = ino.parse().expect("the kernel's directory inode"),
            ["sha", sha] => view.sha = (*sha).to_string(),
            ["link", path, ino, nlink] => view.links.push((
                (*path).to_string(),
                ino.parse().expect("the kernel's inode number"),
                nlink.parse().expect("the kernel's link count"),
            )),
            _ => {}
        }
    }
    assert!(
        view.dir_ino != 0 && !view.sha.is_empty() && !view.links.is_empty(),
        "the kernel's report is incomplete:\n{out}"
    );
    view
}

/// A back-reference as `dump-tree` prints it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct BackRef {
    extref: bool,
    parent: u64,
    name: String,
}

/// Every `INODE_REF` and `INODE_EXTREF` name `ino` has in the fs tree.
///
/// `dump-tree` prints an item as `item N key (ino TYPE offset) ...`, then
/// one line per packed entry: `index I namelen L name: NAME` for an
/// `INODE_REF` (whose key offset is the parent), and `index I parent P
/// namelen L name: NAME` for an `INODE_EXTREF`.
fn back_refs(dump: &str, ino: u64) -> Vec<BackRef> {
    let mut current: Option<(bool, u64)> = None;
    let mut refs = Vec::new();
    for line in dump.lines() {
        let line = line.trim_start();
        if let Some(rest) = line.strip_prefix("item ") {
            current = rest.split_once("key (").and_then(|(_, key)| {
                let mut words = key.trim_end_matches(|c| c != ')').split([' ', ')']);
                let objectid: u64 = words.next()?.parse().ok()?;
                let kind = words.next()?;
                let offset: u64 = words.next()?.parse().ok()?;
                match (objectid == ino, kind) {
                    (true, "INODE_REF") => Some((false, offset)),
                    (true, "INODE_EXTREF") => Some((true, 0)),
                    _ => None,
                }
            });
            continue;
        }
        let Some((extref, key_parent)) = current else {
            continue;
        };
        if !line.starts_with("index ") {
            current = None;
            continue;
        }
        let (fields, name) = line
            .split_once(" name: ")
            .unwrap_or_else(|| panic!("a back-reference with no name: {line:?}"));
        let parent = if extref {
            let words: Vec<&str> = fields.split_whitespace().collect();
            let at = words
                .iter()
                .position(|w| *w == "parent")
                .unwrap_or_else(|| panic!("an INODE_EXTREF entry with no parent: {line:?}"));
            words[at + 1].parse().expect("the extref's parent")
        } else {
            key_parent
        };
        refs.push(BackRef {
            extref,
            parent,
            name: name.to_string(),
        });
    }
    refs
}

/// One step of the sweep: what the kernel does, and which link indices in
/// [`DIR`] exist afterwards.
struct Step {
    what: &'static str,
    script: String,
    present: BTreeSet<usize>,
}

fn link(range: std::ops::Range<usize>) -> String {
    format!(
        "for i in $(seq {} {}); do ln {OTHER} \"{DIR}/$(nm \"$i\")\"; done\n",
        range.start,
        range.end - 1
    )
}

fn unlink(range: std::ops::RangeInclusive<usize>) -> String {
    format!(
        "for i in $(seq {} {}); do rm \"{DIR}/$(nm \"$i\")\"; done\n",
        range.start(),
        range.end()
    )
}

/// Up to the last name the `INODE_REF` holds, one past it, three past it;
/// then one name out of the `INODE_REF`, then every spilled name, which
/// takes the count back down through the boundary.
fn steps(fit: usize) -> Vec<Step> {
    let mut present: BTreeSet<usize> = (0..fit).collect();
    let mut out = vec![Step {
        what: "every name in the INODE_REF",
        script: format!(
            "mkdir {DIR} other\nseq 1 1500 > \"{DIR}/$(nm 0)\"\nln \"{DIR}/$(nm 0)\" {OTHER}\n{}",
            link(1..fit)
        ),
        present: present.clone(),
    }];
    present.insert(fit);
    out.push(Step {
        what: "one name spilled to INODE_EXTREF",
        script: link(fit..fit + 1),
        present: present.clone(),
    });
    present.extend([fit + 1, fit + 2]);
    out.push(Step {
        what: "three names spilled",
        script: link(fit + 1..fit + 3),
        present: present.clone(),
    });
    present.remove(&0);
    out.push(Step {
        what: "an INODE_REF name unlinked, the spill kept",
        script: unlink(0..=0),
        present: present.clone(),
    });
    for i in fit..fit + 3 {
        present.remove(&i);
    }
    out.push(Step {
        what: "every spilled name unlinked",
        script: unlink(fit..=fit + 2),
        present: present.clone(),
    });
    out
}

fn sweep(node_size: usize, name_len: usize) {
    let tag = format!("{}k nodes, {name_len}-byte names", node_size / 1024);
    let fit = names_that_fit(node_size, name_len);

    let image = PathBuf::from(temp_path!("hard-link-extref-{node_size}-{name_len}.img"));
    std::fs::File::create(&image)
        .and_then(|f| f.set_len(256 * 1024 * 1024))
        .expect("creating the image");
    let made = oracle("mkfs.btrfs")
        .args(["-q", "-f", "-O", "extref", "-n", &node_size.to_string()])
        .arg(&image)
        .output();
    assert!(
        made.status.success(),
        "[{tag}] mkfs.btrfs: {}",
        String::from_utf8_lossy(&made.stderr)
    );

    let mut spilled_max = 0;
    for step in steps(fit) {
        let what = format!("{tag}: {}", step.what);
        let script = format!(
            "LEN={name_len}\n{GUEST_PRELUDE}{}{GUEST_REPORT}",
            step.script
        );
        let kernel = parse_kernel(&guest_kernel_write_ok(
            &image.to_string_lossy(),
            &what,
            &script,
        ));
        assert_btrfs_check_clean(&image, &what);

        let names: BTreeSet<String> = step.present.iter().map(|&i| name(i, name_len)).collect();
        let n = u32::try_from(names.len() + 1).unwrap();
        let ino = kernel.links[0].1;

        // The kernel agrees with itself about what it made.
        let kernel_names: BTreeSet<String> = kernel
            .links
            .iter()
            .filter_map(|(path, _, _)| path.strip_prefix(&format!("{DIR}/")))
            .map(str::to_string)
            .collect();
        assert_eq!(
            kernel_names, names,
            "[{what}] the kernel's listing of /{DIR}"
        );
        for (path, link_ino, nlink) in &kernel.links {
            assert_eq!(
                (*link_ino, *nlink),
                (ino, n),
                "[{what}] the kernel's stat of {path}"
            );
        }

        // Where the names landed: the dump shows the boundary was crossed.
        let refs = back_refs(&dump_tree(&image, "5"), ino);
        let in_dir = |extref: bool| -> BTreeSet<String> {
            refs.iter()
                .filter(|r| r.extref == extref && r.parent == kernel.dir_ino)
                .map(|r| r.name.clone())
                .collect()
        };
        let expected = |spilled: bool| -> BTreeSet<String> {
            step.present
                .iter()
                .filter(|&&i| (i >= fit) == spilled)
                .map(|&i| name(i, name_len))
                .collect()
        };
        assert_eq!(
            in_dir(false),
            expected(false),
            "[{what}] the names dump-tree shows in the INODE_REF for /{DIR}"
        );
        assert_eq!(
            in_dir(true),
            expected(true),
            "[{what}] the names dump-tree shows in INODE_EXTREF items for /{DIR}"
        );
        assert_eq!(
            refs.len(),
            names.len() + 1,
            "[{what}] every back-reference the inode has: {refs:?}"
        );
        spilled_max = spilled_max.max(in_dir(true).len());

        // And this crate, read against all of that.
        let fs = Filesystem::mount(Arc::new(FileDevice::open(&image).expect("opening")))
            .unwrap_or_else(|e| panic!("[{what}] mounting: {e}"));
        let dir = fs
            .lookup_path(&format!("/{DIR}"))
            .unwrap_or_else(|e| panic!("[{what}] /{DIR}: {e}"));
        assert_eq!(dir.ino, kernel.dir_ino, "[{what}] /{DIR}'s inode");
        let listed: BTreeSet<String> = fs
            .read_dir(dir.ino)
            .unwrap_or_else(|e| panic!("[{what}] listing /{DIR}: {e}"))
            .into_iter()
            .map(|e| {
                assert_eq!(e.ino, ino, "[{what}] a listed name's inode");
                String::from_utf8(e.name).expect("the names are ASCII")
            })
            .collect();
        assert_eq!(listed, names, "[{what}] this crate's listing of /{DIR}");
        for path in names
            .iter()
            .map(|name| format!("/{DIR}/{name}"))
            .chain([format!("/{OTHER}")])
        {
            let inode = fs
                .lookup_path(&path)
                .unwrap_or_else(|e| panic!("[{what}] {path}: {e}"));
            assert_eq!(inode.ino, ino, "[{what}] {path}'s inode");
            assert_eq!(inode.nlink, n, "[{what}] {path}'s link count");
            let bytes = fs
                .read_file(inode.ino)
                .unwrap_or_else(|e| panic!("[{what}] reading {path}: {e}"));
            assert_eq!(
                sha256_hex(&bytes),
                kernel.sha,
                "[{what}] the bytes read through {path}"
            );
        }
    }

    assert_eq!(
        spilled_max, 3,
        "[{tag}] the sweep never reached three INODE_EXTREF names"
    );
    println!(
        "[kernel vm] {tag}: INODE_REF holds {fit} names, the sweep spilled 3 and unlinked \
         back through it; nlink, names, paths and bytes agree with the kernel at every step"
    );
    let _ = std::fs::remove_file(&image);
}

#[test]
fn links_across_the_spill_with_4k_nodes_and_255_byte_names() {
    sweep(4096, 255);
}

#[test]
fn links_across_the_spill_with_4k_nodes_and_64_byte_names() {
    sweep(4096, 64);
}

#[test]
fn links_across_the_spill_with_16k_nodes_and_255_byte_names() {
    sweep(16384, 255);
}

#[test]
fn the_boundary_moves_with_node_size_and_name_length() {
    // The three sweeps above are only three different tests if these differ.
    assert_eq!(names_that_fit(4096, 255), 14);
    assert_eq!(names_that_fit(4096, 64), 53);
    assert_eq!(names_that_fit(16384, 255), 61);
}

#[test]
fn the_dump_tree_reader_tells_the_two_back_references_apart() {
    // The shape btrfs-progs prints, including an item for another inode and
    // an internal node's key line, neither of which is a back-reference.
    let dump = "\
fs tree key (FS_TREE ROOT_ITEM 0)
node 30556160 level 1 items 2 free space 491 generation 9 owner FS_TREE
\tkey (257 INODE_REF 256) block 30572544 gen 9
leaf 30572544 items 4 free space 100 generation 9 owner FS_TREE
\titem 0 key (257 INODE_ITEM 0) itemoff 3835 itemsize 160
\t\tgeneration 9 transid 9 size 6893 nbytes 8192
\titem 1 key (257 INODE_REF 256) itemoff 3805 itemsize 30
\t\tindex 2 namelen 5 name: first
\t\tindex 3 namelen 6 name: second
\titem 2 key (257 INODE_EXTREF 3405405787) itemoff 3770 itemsize 35
\t\tindex 4 parent 256 namelen 5 name: third
\titem 3 key (258 INODE_REF 256) itemoff 3750 itemsize 20
\t\tindex 5 namelen 5 name: other
";
    let refs = back_refs(dump, 257);
    let named = |extref: bool, name: &str| BackRef {
        extref,
        parent: 256,
        name: name.to_string(),
    };
    assert_eq!(
        refs,
        vec![
            named(false, "first"),
            named(false, "second"),
            named(true, "third")
        ]
    );
}
