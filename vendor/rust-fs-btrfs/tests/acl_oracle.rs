//! POSIX ACLs, read against filesystems the Linux kernel made (#197).
//!
//! An ACL is an extended attribute — `system.posix_acl_access`, and on a
//! directory `system.posix_acl_default` — but its value has a format of
//! its own, a version-2 header and one 8-byte entry per ACL entry, and
//! the kernel refuses a value of the wrong length rather than ignoring
//! it. So the general attribute suite (`tests/xattr_oracle.rs`) does not
//! cover them: nothing there is an ACL, nothing there was written by the
//! kernel's inheritance rather than by `setfattr`, and nothing there is
//! as large as the node size allows.
//!
//! The fixtures are built by `chore fixtures` inside the
//! fs-linux-test-harness VM, one on 4 KiB nodes and one on 16 KiB nodes:
//! ACLs set by `setfacl`, ACLs the kernel applied to files created under
//! a default ACL, and an entry-count sweep up to the largest ACL the node
//! size admits — FOUND by the builder, by bisecting on what the kernel
//! accepts, with the refusal of the next count up recorded beside it.
//! The manifests are the kernel's own account: `getfattr -e hex` for the
//! values and `getfacl` for the entry counts. Every check here is
//! agreement with them, and `btrfs check` in the guest has to find each
//! image clean.
//!
//! Out of scope, and why: this crate cannot set an attribute (its one
//! write path is in-place NODATACOW data, #61), so an ACL written by this
//! crate and read back by the kernel, and inheritance applied by this
//! crate, are not things it can be asked to do yet.

use fs_btrfs::Filesystem;
use fs_btrfs_test_support::{assert_btrfs_check_clean, fixture};
use fs_core::FileDevice;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

const ACCESS: &str = "system.posix_acl_access";
const DEFAULT: &str = "system.posix_acl_default";

/// The two geometries the builder makes, by fixture name.
const GEOMETRIES: [&str; 2] = ["node4k", "node16k"];

/// One fixture: this driver's view of it, and the kernel's.
struct Acls {
    name: &'static str,
    image: PathBuf,
    fs: Filesystem,
    kernel: Manifest,
}

/// What the builder recorded, from `getfattr` and `getfacl` in the guest.
#[derive(Default)]
struct Manifest {
    nodesize: u32,
    /// `access` / `default` -> (largest accepted, the count refused,
    /// setfacl's error for it).
    max: BTreeMap<String, (usize, usize, String)>,
    /// path -> (access entries, default entries), as getfacl counts them.
    count: BTreeMap<String, (usize, usize)>,
    /// (path, name) -> value, from `getfattr -e hex -n <name>`.
    by_name: BTreeMap<(String, String), Vec<u8>>,
    /// path -> {name -> value}, from `getfattr -R -d -m - -e hex`.
    dump: BTreeMap<String, BTreeMap<String, Vec<u8>>>,
}

fn load(name: &'static str) -> Acls {
    let image = fixture(&format!("btrfs-acl-{name}.img"));
    let text = std::fs::read_to_string(fixture(&format!("btrfs-acl-{name}.manifest")))
        .expect("read the ACL manifest");
    let dev = FileDevice::open(&image).expect("open the ACL fixture");
    let fs = Filesystem::mount(Arc::new(dev)).expect("mount the ACL fixture");
    Acls {
        name,
        image,
        fs,
        kernel: parse(&text),
    }
}

fn all() -> Vec<Acls> {
    GEOMETRIES.iter().map(|g| load(g)).collect()
}

fn parse(text: &str) -> Manifest {
    let mut m = Manifest::default();
    let mut current: Option<String> = None;
    for line in text.lines().map(str::trim_end) {
        if let Some(rest) = line.strip_prefix("# acl-nodesize: ") {
            m.nodesize = rest.parse().expect("acl-nodesize");
        } else if let Some(rest) = line.strip_prefix("# acl-max: ") {
            // <kind> <max> refused <next>: <error>
            let (head, error) = rest.split_once(": ").expect("acl-max has an error");
            let f: Vec<&str> = head.split_whitespace().collect();
            assert_eq!(f.len(), 4, "acl-max line: {line}");
            m.max.insert(
                f[0].to_string(),
                (
                    f[1].parse().expect("max"),
                    f[3].parse().expect("refused"),
                    error.to_string(),
                ),
            );
        } else if let Some(rest) = line.strip_prefix("# acl-count: ") {
            // <path> access <n> default <n>
            let f: Vec<&str> = rest.split_whitespace().collect();
            assert_eq!(f.len(), 5, "acl-count line: {line}");
            m.count.insert(
                f[0].to_string(),
                (
                    f[2].parse().expect("access"),
                    f[4].parse().expect("default"),
                ),
            );
        } else if let Some(rest) = line.strip_prefix("# acl-value: ") {
            let f: Vec<&str> = rest.split_whitespace().collect();
            assert_eq!(f.len(), 3, "acl-value line: {line}");
            m.by_name.insert(
                (f[0].to_string(), f[1].to_string()),
                unhex(f[2].strip_prefix("0x").expect("hex value")),
            );
        } else if let Some(rest) = line.strip_prefix("# file: ") {
            let p = format!("/{}", rest.trim_start_matches('.').trim_start_matches('/'));
            m.dump.entry(p.clone()).or_default();
            current = Some(p);
        } else if line.starts_with('#') || line.is_empty() {
            continue;
        } else if let Some(path) = &current {
            let (name, value) = match line.split_once('=') {
                Some((n, v)) => (n.to_string(), unhex(v.strip_prefix("0x").expect("hex"))),
                None => (line.to_string(), Vec::new()),
            };
            m.dump.get_mut(path).expect("inserted").insert(name, value);
        }
    }
    assert!(m.nodesize > 0, "the manifest names no node size");
    m
}

fn unhex(s: &str) -> Vec<u8> {
    assert!(s.len().is_multiple_of(2), "odd-length hex: {s:?}");
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex digit"))
        .collect()
}

impl Acls {
    fn ino(&self, path: &str) -> u64 {
        self.fs
            .lookup_path(path)
            .unwrap_or_else(|e| panic!("{}: lookup {path}: {e}", self.name))
            .ino
    }

    fn get(&self, path: &str, name: &str) -> Option<Vec<u8>> {
        self.fs
            .get_xattr(self.ino(path), name.as_bytes())
            .unwrap_or_else(|e| panic!("{}: get_xattr {path} {name}: {e}", self.name))
    }

    fn list(&self, path: &str) -> BTreeMap<String, Vec<u8>> {
        self.fs
            .list_xattrs(self.ino(path))
            .unwrap_or_else(|e| panic!("{}: list_xattrs {path}: {e}", self.name))
            .into_iter()
            .map(|e| (String::from_utf8_lossy(&e.name).into_owned(), e.value))
            .collect()
    }

    fn max(&self, kind: &str) -> (usize, usize, &str) {
        let (max, refused, error) = self
            .kernel
            .max
            .get(kind)
            .unwrap_or_else(|| panic!("{}: the manifest records no {kind} maximum", self.name));
        (*max, *refused, error)
    }
}

/// The entry count a `posix_acl_xattr` value holds, checked as the
/// kernel's `posix_acl_from_xattr` checks it: a version-2 header, then a
/// whole number of 8-byte entries, each a known tag, in tag order, with
/// exactly one owner, owning group and other entry, and a mask exactly
/// when there is a named entry.
fn acl_entries(value: &[u8], what: &str) -> usize {
    const VERSION: u32 = 2;
    const HEADER: usize = 4;
    const ENTRY: usize = 8;
    const USER_OBJ: u16 = 0x01;
    const USER: u16 = 0x02;
    const GROUP_OBJ: u16 = 0x04;
    const GROUP: u16 = 0x08;
    const MASK: u16 = 0x10;
    const OTHER: u16 = 0x20;

    assert!(
        value.len() >= HEADER,
        "{what}: {} bytes, no header",
        value.len()
    );
    let version = u32::from_le_bytes(value[..HEADER].try_into().unwrap());
    assert_eq!(version, VERSION, "{what}: posix_acl_xattr version");
    assert_eq!(
        (value.len() - HEADER) % ENTRY,
        0,
        "{what}: {} bytes is not a header and whole entries",
        value.len()
    );
    let tags: Vec<u16> = value[HEADER..]
        .chunks_exact(ENTRY)
        .map(|e| u16::from_le_bytes([e[0], e[1]]))
        .collect();
    for t in &tags {
        assert!(
            [USER_OBJ, USER, GROUP_OBJ, GROUP, MASK, OTHER].contains(t),
            "{what}: unknown tag {t:#x}"
        );
    }
    assert!(
        tags.is_sorted(),
        "{what}: entries out of tag order: {tags:x?}"
    );
    let n = |tag| tags.iter().filter(|&&t| t == tag).count();
    assert_eq!(n(USER_OBJ), 1, "{what}: owner entries");
    assert_eq!(n(GROUP_OBJ), 1, "{what}: owning-group entries");
    assert_eq!(n(OTHER), 1, "{what}: other entries");
    let named = n(USER) + n(GROUP);
    assert_eq!(
        n(MASK),
        usize::from(named > 0),
        "{what}: mask entries for {named} named"
    );
    tags.len()
}

/// Every attribute on every path the kernel reports, byte for byte, and
/// no more than it reports — ACLs included — through the listing.
#[test]
fn every_attribute_the_kernel_reports_is_listed_identically() {
    for acls in all() {
        let dumped_acls = acls
            .kernel
            .dump
            .values()
            .flat_map(|m| m.keys())
            .filter(|n| *n == ACCESS || *n == DEFAULT)
            .count();
        assert!(
            dumped_acls >= 20,
            "{}: getfattr listed only {dumped_acls} ACLs — the fixture did not build properly",
            acls.name
        );
        for (path, want) in &acls.kernel.dump {
            assert_eq!(&acls.list(path), want, "{}: {path}: list_xattrs", acls.name);
        }
    }
}

/// Every ACL the kernel returns for `getfattr -n`, byte for byte, from a
/// lookup by name — and a path the kernel reports no ACL for has none
/// here either.
#[test]
fn every_acl_reads_back_by_name_identically() {
    for acls in all() {
        assert!(
            acls.kernel.count.len() >= 20,
            "{}: too few paths",
            acls.name
        );
        for path in acls.kernel.count.keys() {
            for name in [ACCESS, DEFAULT] {
                let want = acls.kernel.by_name.get(&(path.clone(), name.to_string()));
                assert_eq!(
                    acls.get(path, name).as_ref(),
                    want,
                    "{}: {path}: {name}",
                    acls.name
                );
                // The two readings the builder took must agree too, or
                // one of them is not the kernel's answer.
                let dumped = acls.kernel.dump.get(path).and_then(|m| m.get(name));
                assert_eq!(dumped, want, "{}: {path}: {name}: -n vs -d", acls.name);
            }
        }
    }
}

/// Every ACL value is a valid `posix_acl_xattr` whose length implies
/// exactly the entry count `getfacl` reports.
#[test]
fn every_acl_is_well_formed_with_the_count_getfacl_reports() {
    for acls in all() {
        let mut seen = 0;
        for (path, &(access, default)) in &acls.kernel.count {
            let what = format!("{}: {path}", acls.name);
            match acls.get(path, ACCESS) {
                Some(v) => {
                    assert_eq!(acl_entries(&v, &what), access, "{what}: access entries");
                    seen += 1;
                }
                // No attribute: the ACL is the mode bits, which getfacl
                // still shows as three entries.
                None => assert_eq!(access, 3, "{what}: an ACL getfacl sees and we do not"),
            }
            match acls.get(path, DEFAULT) {
                Some(v) => {
                    assert_eq!(acl_entries(&v, &what), default, "{what}: default entries");
                    seen += 1;
                }
                None => assert_eq!(
                    default, 0,
                    "{what}: a default ACL getfacl sees and we do not"
                ),
            }
        }
        assert!(seen >= 20, "{}: only {seen} ACLs checked", acls.name);
    }
}

/// An ACL the mode bits already express is not stored, and one past the
/// largest was refused: both must read as no ACL at all.
#[test]
fn an_acl_the_kernel_did_not_store_is_absent() {
    for acls in all() {
        for path in ["/minimal.txt", "/over.txt", "/over.d"] {
            let names = acls.list(path);
            assert!(
                !names.contains_key(ACCESS) && !names.contains_key(DEFAULT),
                "{}: {path} lists an ACL: {:?}",
                acls.name,
                names.keys().collect::<Vec<_>>()
            );
            assert_eq!(acls.get(path, ACCESS), None, "{}: {path}", acls.name);
        }
        assert_eq!(acls.kernel.count["/minimal.txt"], (3, 0));
    }
}

/// The boundary. Btrfs holds an attribute inline, in one item of one
/// leaf, and refuses (ENOSPC) a name and value that would not fit an item
/// in an empty leaf: `name_len + value_len <= BTRFS_MAX_XATTR_SIZE`,
/// which is the leaf's data area (nodesize less the 101-byte header)
/// less one 25-byte item header and one 30-byte `btrfs_dir_item`.
///
/// The builder FOUND the largest ACL by asking the kernel; this checks
/// that finding against that arithmetic, and that the largest — and the
/// one below it — read back whole. Measured by the builder in the
/// guest: 489 entries at 4 KiB nodes and 2025 at 16 KiB, for access and
/// default ACLs alike (the one-byte longer default name does not cross an
/// 8-byte entry).
#[test]
fn the_largest_acl_the_node_size_admits_reads_back_and_the_next_was_refused() {
    const LEAF_HEADER: usize = 101;
    const ITEM: usize = 25;
    const DIR_ITEM: usize = 30;
    for acls in all() {
        let nodesize = usize::try_from(acls.kernel.nodesize).unwrap();
        assert_eq!(
            acls.fs.superblock().nodesize,
            acls.kernel.nodesize,
            "{}",
            acls.name
        );
        let room = nodesize - LEAF_HEADER - ITEM - DIR_ITEM;
        for (kind, name) in [("access", ACCESS), ("default", DEFAULT)] {
            let (max, refused, error) = acls.max(kind);
            let predicted = (room - name.len() - 4) / 8;
            assert_eq!(
                max, predicted,
                "{}: the kernel's largest {kind} ACL is {max} entries; the item arithmetic \
                 says {predicted}",
                acls.name
            );
            assert_eq!(refused, max + 1, "{}: {kind}", acls.name);
            assert!(
                error.contains("No space left on device"),
                "{}: the {refused}-entry {kind} ACL was refused, but not for room: {error}",
                acls.name
            );
        }

        let (max, _, _) = acls.max("access");
        for n in [max - 1, max] {
            let path = format!("/sweep/entries-{n}.txt");
            let v = acls
                .get(&path, ACCESS)
                .unwrap_or_else(|| panic!("{}: {path}", acls.name));
            assert_eq!(v.len(), 4 + 8 * n, "{}: {path}", acls.name);
            assert_eq!(acl_entries(&v, &path), n);
        }
        // The sweep between: every count the builder set, found by name.
        let sweep: Vec<usize> = acls
            .kernel
            .count
            .iter()
            .filter(|(p, _)| p.starts_with("/sweep/"))
            .map(|(_, &(a, _))| a)
            .collect();
        assert!(
            sweep.contains(&5) && sweep.contains(&max),
            "{}: sweep {sweep:?}",
            acls.name
        );
        assert!(sweep.len() >= 6, "{}: sweep {sweep:?}", acls.name);
    }
}

/// A directory carrying the largest access ACL AND the largest default
/// ACL — on a 4 KiB node, two items that cannot share a leaf — lists
/// both, and what was created under it inherited the largest default.
#[test]
fn the_largest_access_and_default_acls_on_one_directory_both_come_back() {
    for acls in all() {
        let (max_access, _, _) = acls.max("access");
        let (max_default, _, _) = acls.max("default");
        let dir = acls.list("/maxdir");
        assert_eq!(
            acl_entries(&dir[ACCESS], "maxdir"),
            max_access,
            "{}",
            acls.name
        );
        assert_eq!(
            acl_entries(&dir[DEFAULT], "maxdir"),
            max_default,
            "{}",
            acls.name
        );

        let file = acls.list("/maxdir/child.txt");
        assert_eq!(
            acl_entries(&file[ACCESS], "child.txt"),
            max_default,
            "{}",
            acls.name
        );
        assert!(
            !file.contains_key(DEFAULT),
            "{}: a file with a default ACL",
            acls.name
        );

        let sub = acls.list("/maxdir/child.d");
        assert_eq!(
            acl_entries(&sub[ACCESS], "child.d"),
            max_default,
            "{}",
            acls.name
        );
        assert_eq!(
            sub[DEFAULT], dir[DEFAULT],
            "{}: child.d's default",
            acls.name
        );
    }
}

/// Inheritance, as the kernel applied it: a file created under a default
/// ACL gets an access ACL and no default; a directory gets both, its
/// default a copy of its parent's — all the way down.
#[test]
fn acls_the_kernel_inherited_read_back() {
    for acls in all() {
        let parent = acls
            .get("/inherit", DEFAULT)
            .expect("inherit's default ACL");
        let sub = "/inherit/sub";
        assert_eq!(
            acls.get(sub, DEFAULT).as_ref(),
            Some(&parent),
            "{}: {sub}",
            acls.name
        );
        assert!(acls.get(sub, ACCESS).is_some(), "{}: {sub}", acls.name);
        for file in ["/inherit/file.txt", "/inherit/sub/nested.txt"] {
            assert!(acls.get(file, ACCESS).is_some(), "{}: {file}", acls.name);
            assert_eq!(acls.get(file, DEFAULT), None, "{}: {file}", acls.name);
        }
    }
}

/// `btrfs check`, in the guest, finds every ACL fixture clean — so what
/// the rest of this file agrees with is a consistent filesystem.
#[test]
fn every_acl_fixture_is_clean_to_btrfs_check() {
    for acls in all() {
        assert_btrfs_check_clean(&acls.image, &format!("btrfs-acl-{}", acls.name));
    }
}
