//! Extended attributes, read against a filesystem the Linux kernel made.
//!
//! The fixture is built by `chore fixtures`, inside the
//! fs-linux-test-harness VM: a mounted Btrfs filesystem with attributes
//! set by `setfattr`, and `getfattr`'s own dump recorded beside it in
//! `test-disks/`. The dump is the reference answer, so
//! what is checked here is agreement with the kernel rather than
//! agreement with this driver's assumptions — which is the only kind of
//! check worth having for an on-disk layout.
//!
//! The unit tests in `src/xattr.rs` encode items with the same offsets
//! the parser decodes them with, so they cannot tell whether the layout
//! is right at all. This file can.
//!
//! The fixtures are gitignored and generated: `chore fixtures` builds
//! them, and a missing one fails the test that wanted it rather than
//! skipping. A skipped test reads exactly like a passing one, and an
//! xattr suite that never opened an image would report the parser
//! perfect.

use fs_btrfs::Filesystem;
use fs_btrfs_test_support::fixture;
use fs_core::FileDevice;
use std::collections::BTreeMap;
use std::sync::Arc;

/// The mounted fixture and the kernel's dump of it.
fn mounted() -> (Filesystem, Reference) {
    let img = fixture("btrfs-xattr.img");
    let manifest = fixture("btrfs-xattr.manifest");
    let dev = FileDevice::open(&img).expect("open the xattr fixture");
    let fs = Filesystem::mount(Arc::new(dev)).expect("mount the xattr fixture");
    let text = std::fs::read_to_string(&manifest).expect("read the manifest");
    (fs, parse_getfattr(&text))
}

/// What `getfattr` said, as `path -> {name -> value}`.
type Reference = BTreeMap<String, BTreeMap<String, Vec<u8>>>;

/// Parse `getfattr -R -d -m - -e hex` output.
///
/// The format is a `# file: <path>` line followed by `name=0x<hex>`
/// lines, blank-line separated. An attribute whose value is zero-length
/// prints as `name=0x` — getfattr's own spelling for "set, to nothing",
/// which is a different fact from the attribute being absent and has to
/// survive being written down.
///
/// Files with no attributes at all do not appear, so absence in this map
/// means "no attributes", not "unknown".
fn parse_getfattr(text: &str) -> Reference {
    let mut out = Reference::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let line = line.trim_end();
        if let Some(rest) = line.strip_prefix("# file: ") {
            // Paths are recorded relative to the mount point as `./x`;
            // normalise to the absolute form this driver resolves.
            let p = rest.trim_start_matches('.').trim_start_matches('/');
            let p = format!("/{p}");
            out.entry(p.clone()).or_default();
            current = Some(p);
            continue;
        }
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let Some(path) = current.as_ref() else {
            continue;
        };
        let (name, value) = match line.split_once('=') {
            Some((n, v)) => {
                let hex = v
                    .strip_prefix("0x")
                    .unwrap_or_else(|| panic!("getfattr value for {n} is not hex-encoded: {v:?}"));
                (n.to_string(), unhex(hex))
            }
            // getfattr in some versions prints a bare name for an empty
            // value rather than `name=0x`. Accept both spellings.
            None => (line.to_string(), Vec::new()),
        };
        out.get_mut(path)
            .expect("a path we just inserted")
            .insert(name, value);
    }
    out
}

fn unhex(s: &str) -> Vec<u8> {
    assert!(
        s.len().is_multiple_of(2),
        "odd-length hex from getfattr: {s:?}"
    );
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex digit"))
        .collect()
}

/// What this driver says, for one path.
fn ours(fs: &Filesystem, path: &str) -> BTreeMap<String, Vec<u8>> {
    let inode = fs
        .lookup_path(path)
        .unwrap_or_else(|e| panic!("lookup {path}: {e}"));
    fs.list_xattrs(inode.ino)
        .unwrap_or_else(|e| panic!("list_xattrs {path}: {e}"))
        .into_iter()
        .map(|e| (String::from_utf8_lossy(&e.name).into_owned(), e.value))
        .collect()
}

/// Every attribute the kernel reports, byte for byte, on every path it
/// reports one for.
///
/// Compared as maps rather than as lists: Btrfs orders attributes by
/// name hash, `getfattr` orders them its own way, and neither order is a
/// property of the filesystem worth asserting. What must agree is the
/// set of names and every value.
#[test]
fn every_attribute_the_kernel_reports_reads_back_identically() {
    let (fs, reference) = mounted();
    assert!(
        reference.len() >= 4,
        "the manifest names only {} paths — the fixture did not build properly, and \
         comparing against it would prove nothing",
        reference.len()
    );

    for (path, want) in &reference {
        let got = ours(&fs, path);
        assert_eq!(
            got.keys().collect::<Vec<_>>(),
            want.keys().collect::<Vec<_>>(),
            "{path}: the attribute names differ from what getfattr reported"
        );
        for (name, value) in want {
            assert_eq!(
                got.get(name).expect("name checked above"),
                value,
                "{path}: the value of {name} differs from what getfattr reported"
            );
        }
    }
}

/// The case the packed-item parsing exists for.
///
/// Both names hash to the same key, so the kernel stored their records
/// end to end inside one item. A driver reading only the first record
/// returns one attribute and no error at all, which is why this is
/// asserted by name rather than left to the sweep above — a fixture
/// rebuilt without the collision would make that sweep pass while this
/// fails loudly.
#[test]
fn two_names_sharing_one_key_both_come_back() {
    let (fs, _) = mounted();
    let a = "user.tag1371838";
    let b = "user.tag2000402";
    assert_eq!(
        fs_btrfs::dir::name_hash(a.as_bytes()),
        fs_btrfs::dir::name_hash(b.as_bytes()),
        "the fixture's two names no longer collide, so this proves nothing — \
         find another pair and rebuild it"
    );

    let got = ours(&fs, "/collide.txt");
    assert_eq!(
        got.len(),
        2,
        "collide.txt reported {} attributes, not both of the pair: {:?}",
        got.len(),
        got.keys().collect::<Vec<_>>()
    );
    assert_eq!(got[a], b"first of the pair");
    assert_eq!(got[b], b"second of the pair");

    // And each is reachable by name, which goes through the hash rather
    // than the list — the same key, so both lookups land in one item and
    // must then pick the right record out of it.
    let inode = fs.lookup_path("/collide.txt").expect("collide.txt");
    assert_eq!(
        fs.get_xattr(inode.ino, a.as_bytes()).unwrap().as_deref(),
        Some(&b"first of the pair"[..])
    );
    assert_eq!(
        fs.get_xattr(inode.ino, b.as_bytes()).unwrap().as_deref(),
        Some(&b"second of the pair"[..])
    );
}

/// A zero-length value is a value. `getxattr` returning `Some(vec![])`
/// and returning `None` say different things, and a caller acts on the
/// difference.
#[test]
fn an_empty_value_is_not_an_absent_attribute() {
    let (fs, _) = mounted();
    let inode = fs.lookup_path("/plain.txt").expect("plain.txt");
    assert_eq!(
        fs.get_xattr(inode.ino, b"user.empty").unwrap(),
        Some(Vec::new()),
        "an attribute set to a zero-length value read back as absent"
    );
    assert_eq!(
        fs.get_xattr(inode.ino, b"user.never-set").unwrap(),
        None,
        "an attribute that was never set read back as present"
    );
}

/// A file with no attributes lists none — and that must be an empty list
/// rather than an error, since a caller cannot act on the difference
/// between "none" and "could not look".
#[test]
fn a_file_without_attributes_lists_nothing() {
    let (fs, _) = mounted();
    let inode = fs.lookup_path("/bare.txt").expect("bare.txt");
    assert!(fs.list_xattrs(inode.ino).unwrap().is_empty());
    assert_eq!(fs.get_xattr(inode.ino, b"user.colour").unwrap(), None);
}

/// Btrfs stores the namespace prefix as part of the name, so a
/// `trusted.` attribute comes back spelled in full with nothing to
/// expand. The sibling ext4 and EROFS drivers keep prefix tables and
/// would have to rebuild the name; this is the evidence that this one
/// must not.
#[test]
fn a_namespace_other_than_user_survives_intact() {
    let (fs, _) = mounted();
    let inode = fs.lookup_path("/dir/inner.txt").expect("dir/inner.txt");
    let got = fs.list_xattrs(inode.ino).unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].name, b"trusted.root-only");
    assert_eq!(got[0].value, b"only root may set this");
}

/// Directories carry attributes too, and nothing else in the fixture
/// would show it.
#[test]
fn a_directory_carries_attributes() {
    let (fs, _) = mounted();
    let inode = fs.lookup_path("/dir").expect("/dir");
    assert!(inode.is_dir());
    assert_eq!(
        fs.get_xattr(inode.ino, b"user.on-a-directory").unwrap(),
        Some(b"yes".to_vec())
    );
}

/// A binary value is returned unchanged — NUL bytes, high bytes and all.
/// A driver that treated a value as a C string would stop at the first
/// NUL and report a shorter value with no error.
#[test]
fn a_binary_value_survives_byte_for_byte() {
    let (fs, _) = mounted();
    let inode = fs.lookup_path("/plain.txt").expect("plain.txt");
    assert_eq!(
        fs.get_xattr(inode.ino, b"user.binary").unwrap(),
        Some(vec![0x00, 0x01, 0x02, 0xff, 0x7f, 0x0a, 0x00])
    );
}

/// A value long enough that its item is mostly value. The lengths are
/// two independent `u16`s in the same header, and reading one where the
/// other belongs is a mistake a short value would hide.
#[test]
fn a_long_value_comes_back_at_its_full_length() {
    let (fs, _) = mounted();
    let inode = fs.lookup_path("/plain.txt").expect("plain.txt");
    let value = fs
        .get_xattr(inode.ino, b"user.long")
        .unwrap()
        .expect("user.long");
    assert_eq!(value.len(), 2000);
    assert!(value.iter().all(|&b| b == b'x'));
}

/// Asking about an inode that does not exist is a refusal, not an empty
/// list. The two are indistinguishable to a caller otherwise.
#[test]
fn listing_attributes_of_a_missing_inode_is_refused() {
    let (fs, _) = mounted();
    assert!(fs.list_xattrs(u64::MAX / 2).is_err());
    assert!(fs.get_xattr(u64::MAX / 2, b"user.colour").is_err());
}
