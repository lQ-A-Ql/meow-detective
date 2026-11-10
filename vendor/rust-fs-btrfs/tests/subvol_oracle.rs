//! The subvolumes this driver finds must be the ones btrfs-progs
//! reports for the same filesystem.
//!
//! A subvolume listing is easy to produce and hard to produce
//! *correctly*. Walking the root tree and collecting every `ROOT_ITEM`
//! gives a list that looks right and contains the filesystem's internal
//! trees; building paths from names without following parents gives
//! `inner` where the answer is `sub/inner`; and treating a snapshot as
//! an ordinary subvolume gives a listing that is wrong only in the
//! column nobody checks.
//!
//! So the oracle is `btrfs subvolume list`, recorded beside the image
//! when it was built. It names every subvolume, gives its id and path,
//! and says which are snapshots — and it is what a user would compare
//! against.
//!
//! The fixture and its manifest are gitignored and built by `chore
//! fixtures`, which mounts the filesystem in the fs-linux-test-harness
//! VM and records what btrfs-progs says about it. A missing one fails
//! here rather than skipping: this suite skipped on the run that added
//! it, and the CI workflow carried a note saying so for months.

use fs_btrfs::fs::Filesystem;
use fs_btrfs::subvol::FS_TREE_OBJECTID;
use fs_btrfs_test_support::fixture;
use fs_core::FileDevice;
use std::collections::BTreeMap;
use std::sync::Arc;

/// One line of `btrfs subvolume list -pcgu`, reduced to what is being
/// compared.
#[derive(Debug, PartialEq, Eq)]
struct Reported {
    id: u64,
    parent: u64,
    path: String,
}

/// Parse the manifest btrfs-progs wrote.
///
/// Its lines look like:
///
/// ```text
/// ID 256 gen 12 cgen 7 parent 5 top level 5 uuid ... path sub
/// ```
fn reference() -> Vec<Reported> {
    let text = manifest();
    let mut out = Vec::new();
    for line in text.lines() {
        if !line.starts_with("ID ") {
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        // A line that starts `ID ` and then lacks a field is a manifest
        // btrfs-progs did not write, which is a broken fixture rather
        // than a line to pass over.
        let field = |name: &str| -> String {
            f.iter()
                .position(|w| *w == name)
                .and_then(|i| f.get(i + 1))
                .unwrap_or_else(|| panic!("the manifest line {line:?} has no {name} field"))
                .to_string()
        };
        let number = |name: &str| -> u64 {
            field(name)
                .parse()
                .unwrap_or_else(|e| panic!("{name} in the manifest line {line:?}: {e}"))
        };
        out.push(Reported {
            id: number("ID"),
            parent: number("parent"),
            // `path` is the last field, and a path may contain no spaces
            // in any filesystem this builds.
            path: field("path"),
        });
    }
    out
}

/// The manifest btrfs-progs wrote beside the image.
fn manifest() -> String {
    std::fs::read_to_string(fixture("btrfs-subvol.manifest")).expect("read the manifest")
}

/// Which files each subvolume holds, as the manifest recorded them.
///
/// Never empty: a manifest with no `contains` lines used to leave two
/// tests below printing a note and returning, which reads as a pass.
fn contents() -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    let text = manifest();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("contains ") else {
            continue;
        };
        let Some((path, files)) = rest.split_once(':') else {
            continue;
        };
        out.insert(
            path.to_string(),
            files.split_whitespace().map(str::to_string).collect(),
        );
    }
    assert!(
        !out.is_empty(),
        "the manifest records no subvolume contents, so there is nothing to \
         compare the subvolumes against"
    );
    out
}

fn mount() -> Filesystem {
    let img = fixture("btrfs-subvol.img");
    let dev = FileDevice::open(&img).expect("open the subvolume fixture");
    Filesystem::mount(Arc::new(dev)).expect("mount the subvolume fixture")
}

/// Every subvolume btrfs-progs reported, and no others.
#[test]
fn the_listing_matches_btrfs_progs() {
    let (fs, expected) = (mount(), reference());
    assert!(
        expected.len() >= 4,
        "the fixture should hold several subvolumes, not {}",
        expected.len()
    );

    let ours = fs.subvolumes().expect("list the subvolumes");

    // btrfs-progs does not list the default subvolume — it is where the
    // listing is taken from — so it is compared separately.
    let default: Vec<_> = ours.iter().filter(|s| s.is_default()).collect();
    assert_eq!(
        default.len(),
        1,
        "there is exactly one default subvolume, and it is always present"
    );
    assert_eq!(default[0].id, FS_TREE_OBJECTID);
    assert_eq!(default[0].path, "/");
    assert!(
        default[0].name.is_empty(),
        "the default subvolume has no name to have"
    );

    let mut got: Vec<Reported> = ours
        .iter()
        .filter(|s| !s.is_default())
        .map(|s| Reported {
            id: s.id,
            parent: s.parent,
            path: s.path.clone(),
        })
        .collect();
    got.sort_by_key(|r| r.id);

    let mut want = expected;
    want.sort_by_key(|r| r.id);

    assert_eq!(
        got, want,
        "the subvolumes found do not match what btrfs-progs reported.\n \
         ours: {got:#?}\n btrfs: {want:#?}"
    );

    eprintln!(
        "{} subvolumes match btrfs-progs, plus the default one: {:?}",
        got.len(),
        got.iter().map(|r| &r.path).collect::<Vec<_>>()
    );
}

/// A nested subvolume's path is its whole chain, not just its own name.
///
/// The fixture puts `inner` inside `sub` for exactly this: a listing
/// built from names alone reports `inner`, which is a different and
/// non-existent path.
#[test]
fn a_nested_subvolume_carries_its_parents_path() {
    let fs = mount();
    let subs = fs.subvolumes().expect("list");

    let inner = subs
        .iter()
        .find(|s| s.name == b"inner")
        .expect("the fixture has a nested subvolume");
    assert_eq!(
        inner.path, "sub/inner",
        "a nested subvolume's path is the chain, not its own name"
    );

    let sub = subs
        .iter()
        .find(|s| s.name == b"sub")
        .expect("the fixture has the parent");
    assert_eq!(inner.parent, sub.id, "and its parent is that subvolume");
}

/// Snapshots are distinguished from subvolumes, and read-only ones from
/// writable ones.
///
/// Both are columns a listing can get wrong while looking entirely
/// plausible. The fixture takes two snapshots of the same subvolume, one
/// read-only, so each flag is exercised against a near-identical
/// neighbour rather than against something obviously different.
#[test]
fn snapshots_and_read_only_are_told_apart() {
    let fs = mount();
    let subs = fs.subvolumes().expect("list");
    let by_name = |n: &[u8]| {
        subs.iter()
            .find(|s| s.name == n)
            .unwrap_or_else(|| panic!("the fixture has {}", String::from_utf8_lossy(n)))
    };

    let sub = by_name(b"sub");
    let snap = by_name(b"snap");
    let rosnap = by_name(b"rosnap");

    assert!(!sub.is_snapshot, "`sub` was created empty, not snapshotted");
    assert!(snap.is_snapshot, "`snap` is a snapshot of `sub`");
    assert!(rosnap.is_snapshot, "`rosnap` is too");

    assert!(!sub.read_only);
    assert!(!snap.read_only, "`snap` was taken writable");
    assert!(
        rosnap.read_only,
        "`rosnap` was taken with -r and is the only read-only one"
    );

    // A snapshot and its parent are different trees. They share blocks
    // when taken, but the fixture writes to `sub` afterwards, so by now
    // they must point at different roots — a driver that resolved a
    // snapshot to its parent's current tree would report the same.
    assert_ne!(
        snap.bytenr, sub.bytenr,
        "`sub` was written to after `snap` was taken, so their trees have diverged"
    );

    eprintln!(
        "snapshot and read-only flags agree with how the fixture was built \
         (sub {}, snap {}, rosnap {})",
        sub.bytenr, snap.bytenr, rosnap.bytenr
    );
}

/// The listing does not report the filesystem's internal trees.
///
/// The root tree of an ordinary filesystem holds trees numbered
/// negatively, which read as enormous unsigned values — so a filter with
/// only a lower bound admits them and reports an internal tree as a
/// subvolume.
#[test]
fn internal_trees_stay_out_of_the_listing() {
    let fs = mount();

    let all = fs.root_tree_items().expect("walk the root tree");
    let root_items = all.iter().filter(|(_, t, _, _)| *t == 132).count();
    let listed = fs.subvolumes().expect("list").len();

    assert!(
        root_items > listed,
        "the root tree holds {root_items} trees and the listing reports {listed}; \
         if they are equal the internal trees are being reported as subvolumes"
    );

    for s in fs.subvolumes().expect("list") {
        assert!(
            s.id == FS_TREE_OBJECTID || s.id >= 256,
            "id {} is not in the range a subvolume is numbered from",
            s.id
        );
        assert!(
            s.id < u64::MAX - 255,
            "id {} is a negatively numbered internal tree",
            s.id
        );
    }

    eprintln!("{root_items} trees in the root tree, {listed} of them subvolumes");
}

/// Each subvolume's tree really is distinct, and the manifest says what
/// each should hold.
///
/// This does not yet read *inside* a subvolume — that needs the
/// filesystem to be re-rooted at the subvolume's tree, which is the next
/// piece. What it checks is that the roots differ, which is the
/// precondition for that being worth doing at all.
#[test]
fn every_subvolume_has_a_root_of_its_own() {
    let fs = mount();
    let subs = fs.subvolumes().expect("list");
    let expected = contents();

    let mut seen: BTreeMap<u64, u64> = BTreeMap::new();
    for s in &subs {
        if let Some(other) = seen.insert(s.bytenr, s.id) {
            panic!(
                "subvolumes {} and {} share a root block ({}), so one of them is \
                 being resolved to the other's tree",
                other, s.id, s.bytenr
            );
        }
        assert_ne!(s.bytenr, 0, "subvolume {} has no root block", s.id);
    }

    eprintln!(
        "{} subvolumes, {} distinct roots; the manifest expects {:?}",
        subs.len(),
        seen.len(),
        expected.keys().collect::<Vec<_>>()
    );
}

/// Reading inside a subvolume gives that subvolume's contents.
///
/// The manifest recorded what each one holds, taken from the mounted
/// filesystem with `find`. This opens each and compares.
///
/// # The case the whole fixture is built around
///
/// `sub` gained `after.txt` AFTER `snap` was taken. So a driver that
/// resolved a snapshot to its parent's *current* tree — the easy
/// mistake, since a snapshot shares its parent's blocks when taken —
/// reads `after.txt` inside `snap`, where it has never existed.
///
/// Counting names would not catch that. Comparing them does.
#[test]
fn each_subvolume_reads_its_own_contents() {
    let fs = mount();
    let expected = contents();

    let subs = fs.subvolumes().expect("list");
    let mut checked = 0usize;

    for s in &subs {
        // `top` is a directory in the default subvolume rather than a
        // subvolume of its own, so it is checked through the default
        // handle below.
        let Some(want) = expected.get(&s.path) else {
            continue;
        };

        let view = fs
            .open_subvolume(s.id)
            .unwrap_or_else(|e| panic!("opening subvolume {} ({}): {e}", s.id, s.path));

        let root = view.root_inode().expect("its root");
        let mut got: Vec<String> = view
            .read_dir(root.ino)
            .unwrap_or_else(|e| panic!("listing {}: {e}", s.path))
            .into_iter()
            .filter(|e| e.is_inode())
            .map(|e| String::from_utf8_lossy(&e.name).into_owned())
            .collect();
        got.sort();

        let mut want = want.clone();
        want.sort();

        assert_eq!(
            got, want,
            "subvolume {:?} holds {got:?}, the manifest recorded {want:?}",
            s.path
        );
        checked += 1;
    }

    assert!(
        checked >= 3,
        "only {checked} subvolumes were compared against the manifest"
    );

    // Stated separately because it is the point of the fixture: the
    // snapshot must NOT hold what its parent gained afterwards.
    let snap = subs.iter().find(|s| s.name == b"snap").expect("snap");
    let view = fs.open_subvolume(snap.id).expect("open snap");
    let root = view.root_inode().expect("root");
    let names: Vec<String> = view
        .read_dir(root.ino)
        .expect("list")
        .into_iter()
        .map(|e| String::from_utf8_lossy(&e.name).into_owned())
        .collect();

    assert!(
        names.iter().any(|n| n == "b.txt"),
        "the snapshot should hold what its parent held when it was taken: {names:?}"
    );
    assert!(
        !names.iter().any(|n| n == "after.txt"),
        "the snapshot holds a file its parent gained AFTER it was taken, so it is \
         being resolved to the parent's current tree rather than its own: {names:?}"
    );

    eprintln!(
        "{checked} subvolumes read their own contents; the snapshot correctly \
         lacks the file its parent gained afterwards"
    );
}

/// A file inside a subvolume reads back with the right bytes.
///
/// Listing names proves the tree is the right one; reading a file proves
/// the extents in it resolve against the same chunk map, which is shared
/// with the parent filesystem and is the piece most likely to be wired
/// up wrongly when a handle is re-rooted.
#[test]
fn a_file_inside_a_subvolume_reads_back() {
    let fs = mount();
    let subs = fs.subvolumes().expect("list");
    let sub = subs.iter().find(|s| s.name == b"sub").expect("sub");

    let view = fs.open_subvolume(sub.id).expect("open sub");
    let inode = view
        .lookup_path("/b.txt")
        .expect("the file the fixture wrote into `sub`");
    let bytes = view.read_file(inode.ino).expect("read it");

    assert_eq!(
        String::from_utf8_lossy(&bytes).trim(),
        "in sub",
        "the file inside the subvolume did not read back what was written to it"
    );

    // And the same path does not exist in the default subvolume, which
    // is what says the two handles really are looking at different
    // trees.
    assert!(
        fs.lookup_path("/b.txt").is_err(),
        "`/b.txt` exists only inside `sub`, so the default tree must not find it"
    );

    eprintln!(
        "read {:?} from inside `sub`",
        String::from_utf8_lossy(&bytes).trim()
    );
}

/// A path that crosses subvolume boundaries reads what the kernel reads
/// there (#62).
///
/// `/sub/inner/c.txt` crosses two, into `sub` and then into `inner`.
/// `/snap` is a snapshot of `sub` taken while `inner` existed, and a
/// snapshot does not carry nested subvolumes. Its `inner` entry still
/// names subvolume `inner`, but no `ROOT_REF` from `snap` backs it, so the
/// kernel shows an empty directory with inode 2 in its place, not
/// `inner`'s files. Both facts were measured on a mount of this fixture
/// (Linux 6.12).
#[test]
fn a_path_crosses_into_subvolumes() {
    let fs = mount();
    for (path, want) in [
        ("/top/a.txt", "in the default subvolume\n"),
        ("/sub/b.txt", "in sub\n"),
        ("/sub/inner/c.txt", "in sub/inner\n"),
        ("/snap/b.txt", "in sub\n"),
        ("/rosnap/b.txt", "in sub\n"),
    ] {
        let got = fs.read_path(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_eq!(String::from_utf8_lossy(&got), want, "{path}");
    }

    let target = fs.resolve_path("/sub/inner/c.txt").expect("resolve");
    assert_eq!(
        target.fs(&fs).read_file(target.inode.ino).expect("read"),
        b"in sub/inner\n",
        "the inode must be read in the tree the path ended in"
    );
    let names: Vec<Vec<u8>> = fs
        .list_path("/sub/inner")
        .expect("list /sub/inner")
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, vec![b"c.txt".to_vec()]);

    assert!(
        matches!(
            fs.read_path("/snap/after.txt"),
            Err(fs_btrfs::error::Error::NotFound)
        ),
        "the snapshot must not hold what its source gained afterwards"
    );

    let stub = fs.resolve_path("/snap/inner").expect("resolve /snap/inner");
    assert_eq!(stub.inode.ino, 2, "the kernel's empty-subvolume directory");
    assert!(stub.inode.is_dir());
    assert!(
        fs.list_path("/snap/inner")
            .expect("list the stub")
            .is_empty(),
        "a snapshot does not carry the subvolume nested in its source"
    );
    assert!(
        matches!(
            fs.read_path("/snap/inner/c.txt"),
            Err(fs_btrfs::error::Error::NotFound)
        ),
        "the nested subvolume's file must not be reachable through the snapshot"
    );
}
