//! Subvolumes and snapshots.
//!
//! Every fixture before this one had exactly one subvolume — the default
//! fs tree, objectid 5 — which is the only shape the driver had ever
//! been asked to read and is not the shape a real Btrfs filesystem has.
//! Subvolumes are how people use Btrfs, and a snapshot is how they back
//! it up.
//!
//! # Where they live
//!
//! The root tree is the index of trees. Walking it gives three kinds of
//! item that matter here, and what each one is was read off a filesystem
//! rather than assumed:
//!
//! ```text
//! type 132  ROOT_ITEM      one per tree: where its root block is, its
//!                          generation, its flags
//! type 156  ROOT_REF       key (parent, 156, child), data ends in the name
//! type 144  ROOT_BACKREF   key (child, 144, parent), the same name
//! ```
//!
//! `ROOT_BACKREF` is the one used here: one item per subvolume giving
//! its parent and its name, which is exactly what building a path needs.
//! `ROOT_REF` says the same thing from the other end.
//!
//! # Four things that were measured
//!
//! **A snapshot is a `ROOT_ITEM` whose key offset is not zero.** On a
//! filesystem with two subvolumes and two snapshots, the two snapshots
//! read 10 and 11 — the generation each was created at — and everything
//! else read 0.
//!
//! **Read-only is `flags` bit 0**, at offset 208 in the item. Of five
//! subvolumes on that filesystem, exactly the one created with
//! `btrfs subvolume snapshot -r` had it set.
//!
//! **The name is 18 bytes into the reference item.** A `dirid` and a
//! `sequence`, both 64-bit, then a 16-bit length. Confirmed on four
//! names of different lengths: `sub` in a 21-byte item, `snap` in 22,
//! `inner` in 23, `rosnap` in 24.
//!
//! **Some objectids are negative.** The root tree of that same
//! filesystem holds one numbered 18446744073709551607, which is −9 read
//! as unsigned. A filter of "256 or above" admits it; the range has an
//! upper end as well as a lower one, and [`LAST_FREE_OBJECTID`] is it.

use crate::error::Result;
use crate::fs::Filesystem;

use crate::fs::ROOT_ITEM_KEY;

/// `BTRFS_ROOT_BACKREF_KEY` — a subvolume's own record of its parent and
/// its name.
const ROOT_BACKREF_KEY: u8 = 144;

/// `BTRFS_FS_TREE_OBJECTID` — the default subvolume, which every
/// filesystem has and which has no name and no parent.
pub use crate::chunk::objectid::FS_TREE as FS_TREE_OBJECTID;

/// `BTRFS_FIRST_FREE_OBJECTID` — the lowest id a created subvolume gets.
pub use crate::inode::FIRST_FREE_OBJECTID;

/// `BTRFS_LAST_FREE_OBJECTID` — the highest, and the reason this range
/// has two ends.
///
/// Btrfs numbers several internal trees with *negative* objectids, which
/// read as very large unsigned values. One such tree sits in the root
/// tree of an ordinary filesystem, so "256 or above" is not a filter,
/// it is a filter that admits an internal tree and reports it as a
/// subvolume.
pub const LAST_FREE_OBJECTID: u64 = u64::MAX - 255;

/// Byte offsets within `struct btrfs_root_item`.
///
/// The item opens with an embedded 160-byte `btrfs_inode_item`, then a
/// generation and a `root_dirid` before the address of the root block.
use crate::fs::root_item;

/// Byte offsets within `struct btrfs_root_ref`, which both the reference
/// and the back-reference use.
mod root_ref {
    /// `u16`. How many bytes of name follow.
    pub const NAME_LEN: usize = 16;
    /// The name itself.
    pub const NAME: usize = 18;
}

/// `BTRFS_ROOT_SUBVOL_RDONLY`.
const ROOT_SUBVOL_RDONLY: u64 = 1 << 0;

/// `ROOT_REF`: filed in the root tree under the parent tree's id, with the
/// child's id as the offset, saying which directory of the parent holds
/// the child and by what name.
const ROOT_REF_KEY: u8 = 156;

/// `BTRFS_EMPTY_SUBVOL_DIR_OBJECTID`: the inode number the kernel gives
/// the empty directory it shows where a subvolume entry has no
/// `ROOT_REF` behind it. That is what a snapshot's copy of a nested
/// subvolume's entry looks like, because a snapshot does not carry the
/// subvolumes nested in its source. No tree holds an inode with this
/// number.
pub const EMPTY_SUBVOL_DIR_OBJECTID: u64 = 2;

/// Where a path ends: an inode, and the tree it is an inode of.
pub struct PathTarget {
    /// The subvolume the path crossed into last, or `None` when it
    /// crossed none and the inode belongs to the handle it started from.
    pub tree: Option<Filesystem>,
    /// The inode, meaningful only in [`PathTarget::fs`].
    pub inode: crate::inode::Inode,
}

impl PathTarget {
    /// The handle to read [`inode`](Self::inode) through, given the one
    /// the path was resolved from.
    pub fn fs<'a>(&'a self, start: &'a Filesystem) -> &'a Filesystem {
        self.tree.as_ref().unwrap_or(start)
    }
}

/// One subvolume or snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subvolume {
    /// Its objectid, which is what `btrfs subvolume list` calls the ID.
    pub id: u64,
    /// The subvolume it was created inside, or zero for the default one.
    pub parent: u64,
    /// Its name within its parent. Empty for the default subvolume,
    /// which has none.
    pub name: Vec<u8>,
    /// Its path from the top of the filesystem, as
    /// `btrfs subvolume list` prints it — `sub/inner` rather than
    /// `inner`.
    pub path: String,
    /// The root block of its tree.
    pub bytenr: u64,
    /// The transaction it was last written in.
    pub generation: u64,
    /// The generation it was last snapshotted at, or zero.
    pub last_snapshot: u64,
    /// Whether it was created read-only.
    pub read_only: bool,
    /// Whether it is a snapshot of another subvolume rather than a
    /// subvolume created empty.
    pub is_snapshot: bool,
}

impl Subvolume {
    /// Whether this is the default subvolume, which is present on every
    /// filesystem and is nobody's child.
    pub fn is_default(&self) -> bool {
        self.id == FS_TREE_OBJECTID
    }
}

/// Whether an objectid names a subvolume rather than one of the
/// filesystem's internal trees.
///
/// The default subvolume, or one in the free range — which has an upper
/// end because some internal trees are numbered negatively and read as
/// enormous unsigned values.
pub fn is_subvolume_id(objectid: u64) -> bool {
    objectid == FS_TREE_OBJECTID || (FIRST_FREE_OBJECTID..=LAST_FREE_OBJECTID).contains(&objectid)
}

impl Filesystem {
    /// Every subvolume and snapshot on the filesystem, by ascending id.
    ///
    /// The default subvolume comes first, as id 5, with an empty name
    /// and a path of `/`. The rest carry the path
    /// `btrfs subvolume list` would print.
    ///
    /// # Errors
    ///
    /// As the B-tree walk.
    pub fn subvolumes(&self) -> Result<Vec<Subvolume>> {
        let items = self.root_tree_items()?;

        // Two passes, because a path cannot be built until every name is
        // known: a nested subvolume names only its immediate parent, and
        // the parent may be read after the child.
        let mut names: std::collections::BTreeMap<u64, (u64, Vec<u8>)> =
            std::collections::BTreeMap::new();
        for (objectid, key_type, offset, data) in &items {
            if *key_type != ROOT_BACKREF_KEY || !is_subvolume_id(*objectid) {
                continue;
            }
            let Some(name) = reference_name(data) else {
                continue;
            };
            names.insert(*objectid, (*offset, name));
        }

        let mut out = Vec::new();
        for (objectid, key_type, offset, data) in &items {
            if *key_type != ROOT_ITEM_KEY
                || !is_subvolume_id(*objectid)
                || data.len() < root_item::MIN_SIZE
            {
                continue;
            }
            let le64 =
                |at: usize| u64::from_le_bytes(data[at..at + 8].try_into().expect("8 bytes"));

            let (parent, name) = names.get(objectid).cloned().unwrap_or((0, Vec::new()));

            out.push(Subvolume {
                id: *objectid,
                parent,
                path: path_of(*objectid, &names),
                name,
                bytenr: le64(root_item::BYTENR),
                generation: le64(root_item::GENERATION),
                last_snapshot: le64(root_item::LAST_SNAPSHOT),
                read_only: le64(root_item::FLAGS) & ROOT_SUBVOL_RDONLY != 0,
                // A snapshot records the generation it was taken at in
                // its key's offset; a subvolume created empty has zero.
                is_snapshot: *offset != 0,
            });
        }

        out.sort_by_key(|s| s.id);
        Ok(out)
    }
}

/// The name out of a `ROOT_REF` or `ROOT_BACKREF` item.
///
/// `None` rather than an error for an item too short to hold what it
/// claims: a listing that skipped one malformed reference is more useful
/// than one that refused to produce anything.
fn reference_name(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < root_ref::NAME {
        return None;
    }
    let len = u16::from_le_bytes(
        data[root_ref::NAME_LEN..root_ref::NAME_LEN + 2]
            .try_into()
            .expect("2 bytes"),
    ) as usize;
    let end = root_ref::NAME.checked_add(len)?;
    if end > data.len() {
        return None;
    }
    Some(data[root_ref::NAME..end].to_vec())
}

/// The path a subvolume is reached by, built from its chain of parents.
///
/// Bounded rather than recursive: a filesystem whose references form a
/// cycle would otherwise hang here, and a listing is exactly the tool
/// someone reaches for when they suspect something is wrong.
fn path_of(id: u64, names: &std::collections::BTreeMap<u64, (u64, Vec<u8>)>) -> String {
    if id == FS_TREE_OBJECTID {
        return "/".to_string();
    }

    let mut parts: Vec<String> = Vec::new();
    let mut at = id;
    for _ in 0..names.len() + 1 {
        let Some((parent, name)) = names.get(&at) else {
            break;
        };
        parts.push(String::from_utf8_lossy(name).into_owned());
        if *parent == FS_TREE_OBJECTID {
            break;
        }
        at = *parent;
    }
    parts.reverse();
    parts.join("/")
}

impl Filesystem {
    /// Open one subvolume as a filesystem of its own.
    ///
    /// A subvolume is a separate tree over the same device. Everything
    /// below the tree — the chunk map, the superblock, the device — is
    /// shared; only which root the item cache is loaded from differs. So
    /// the returned handle answers `read_dir`, `lookup`, `read_file` and
    /// the rest against that subvolume, and paths inside it are absolute
    /// within it rather than carrying its own path as a prefix.
    ///
    /// # Read-only, deliberately
    ///
    /// The handle never carries the write capability, even when this one
    /// does. Writing into a subvolume is not the same operation as
    /// writing into the default tree — a snapshot shares its blocks with
    /// its parent until something writes, so a write has to unshare
    /// before it can proceed, and none of that is implemented. A handle
    /// that could not write is better than one that could write wrongly.
    ///
    /// # Errors
    ///
    /// [`crate::error::Error::NotFound`] if no subvolume has that id, and whatever the
    /// tree walk returns.
    pub fn open_subvolume(&self, id: u64) -> Result<Filesystem> {
        let subvol = self
            .subvolumes()?
            .into_iter()
            .find(|s| s.id == id)
            .ok_or(crate::error::Error::NotFound)?;
        self.open_subvolume_at(subvol.bytenr)
    }

    /// Open the subvolume whose tree root is `bytenr`.
    ///
    /// Separate from [`Filesystem::open_subvolume`] so a caller that has
    /// already listed them does not pay for a second walk of the root
    /// tree to look up something it is holding.
    ///
    /// # Errors
    ///
    /// As the tree walk. A `bytenr` that is not a tree root fails there
    /// rather than producing an empty filesystem, because a tree block
    /// carries its own identity and the walk checks it.
    pub fn open_subvolume_at(&self, bytenr: u64) -> Result<Filesystem> {
        self.reroot(bytenr)
    }

    /// Resolve an absolute path to its inode, crossing into every
    /// subvolume on the way, and hand back the tree the inode belongs to.
    ///
    /// A crossing follows the kernel's `fixup_tree_root_location`. The
    /// entry names a subvolume, and a `ROOT_REF` from this tree must name
    /// the same directory and name. Then the path continues at that
    /// subvolume's top directory. Without a `ROOT_REF` the kernel shows an
    /// empty directory, inode [`EMPTY_SUBVOL_DIR_OBJECTID`], and so does
    /// this; nothing is reachable beneath it.
    ///
    /// Symbolic links are not followed, as in
    /// [`lookup_path`](Filesystem::lookup_path). Every crossed-into handle
    /// is read-only; see [`open_subvolume`](Filesystem::open_subvolume).
    ///
    /// # Errors
    ///
    /// [`Error::NotFound`] for a missing component,
    /// [`Error::NotADirectory`] for a component under a non-directory,
    /// and whatever the tree walks return.
    ///
    /// [`Error::NotFound`]: crate::error::Error::NotFound
    /// [`Error::NotADirectory`]: crate::error::Error::NotADirectory
    pub fn resolve_path(&self, path: &str) -> Result<PathTarget> {
        self.resolve_path_bytes(path.as_bytes())
    }

    /// [`resolve_path`] with the path given as bytes.
    ///
    /// The real one; the `&str` form above is the convenience. Btrfs
    /// directory entry names are raw bytes with no encoding rule, so a
    /// component is compared byte for byte against the on-disk entry and
    /// nothing is decoded on the way (#214).
    ///
    /// [`resolve_path`]: Self::resolve_path
    pub fn resolve_path_bytes(&self, path: &[u8]) -> Result<PathTarget> {
        use crate::error::Error;
        let mut tree: Option<Filesystem> = None;
        let mut inode = self.root_inode()?;
        for component in path
            .split(|&b| b == b'/')
            .filter(|c| !c.is_empty() && *c != b".")
        {
            if component == b".." {
                return Err(Error::UnsupportedFeature(
                    "`..` in a path is not resolved by resolve_path".into(),
                ));
            }
            if !inode.is_dir() {
                return Err(Error::NotADirectory);
            }
            if inode.ino == EMPTY_SUBVOL_DIR_OBJECTID {
                return Err(Error::NotFound);
            }
            let here = tree.as_ref().unwrap_or(self);
            let entry = here.lookup_entry(inode.ino, component)?;
            if entry.is_inode() {
                inode = here.read_inode(entry.ino)?;
                continue;
            }
            match here.subvolume_behind(inode.ino, &entry)? {
                Some((bytenr, dirid)) => {
                    let child = here.reroot(bytenr)?;
                    inode = child.read_inode(dirid)?;
                    tree = Some(child);
                }
                None => inode = empty_subvolume_dir(&inode),
            }
        }
        Ok(PathTarget { tree, inode })
    }

    /// The root block and top directory of the subvolume `entry` names,
    /// when a `ROOT_REF` from this tree backs it.
    ///
    /// This handle knows its tree by root block, not by id, so the parent
    /// id is found as the tree whose `ROOT_ITEM` points at that block.
    fn subvolume_behind(
        &self,
        dir_ino: u64,
        entry: &crate::dir::DirEntry,
    ) -> Result<Option<(u64, u64)>> {
        use crate::error::Error;
        let reader = self.pool_reader();
        let root_tree = reader.tree();
        let root_item_of = |id: u64| -> Result<Option<Vec<u8>>> {
            Ok(root_tree
                .find_all(self.sb.root, id, ROOT_ITEM_KEY)?
                .into_iter()
                .next()
                .map(|item| item.data))
        };
        let Some(child) = root_item_of(entry.ino)? else {
            return Ok(None);
        };
        if child.len() < root_item::MIN_SIZE {
            return Err(Error::BadSuperblock(format!(
                "subvolume {}: ROOT_ITEM is {} bytes",
                entry.ino,
                child.len()
            )));
        }
        let le64 = |data: &[u8], at: usize| {
            u64::from_le_bytes(data[at..at + 8].try_into().expect("8 bytes"))
        };
        for backref in root_tree.find_all(self.sb.root, entry.ino, ROOT_BACKREF_KEY)? {
            let parent = backref.key.offset;
            let same_tree = root_item_of(parent)?.is_some_and(|data| {
                data.len() >= root_item::MIN_SIZE
                    && le64(&data, root_item::BYTENR) == self.fs_tree_root
            });
            if !same_tree {
                continue;
            }
            let backed = root_tree
                .search(
                    self.sb.root,
                    &crate::chunk::DiskKey {
                        objectid: parent,
                        key_type: ROOT_REF_KEY,
                        offset: entry.ino,
                    },
                )?
                .is_some_and(|item| {
                    item.data.len() >= 8
                        && le64(&item.data, 0) == dir_ino
                        && reference_name(&item.data).as_deref() == Some(&entry.name[..])
                });
            if backed {
                return Ok(Some((
                    le64(&child, root_item::BYTENR),
                    le64(&child, root_item::ROOT_DIRID),
                )));
            }
        }
        Ok(None)
    }
}

/// The directory the kernel shows for a subvolume entry with no
/// `ROOT_REF` behind it (`new_simple_dir`): mode 0755, one link, and every
/// time taken from the containing directory's change time.
fn empty_subvolume_dir(dir: &crate::inode::Inode) -> crate::inode::Inode {
    crate::inode::Inode {
        ino: EMPTY_SUBVOL_DIR_OBJECTID,
        size: 0,
        nbytes: 0,
        nlink: 1,
        uid: 0,
        gid: 0,
        mode: 0o040755,
        rdev: 0,
        flags: 0,
        atime: dir.ctime,
        mtime: dir.ctime,
        ctime: dir.ctime,
        otime: dir.ctime,
        ..dir.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The id range has two ends, and the upper one is the point.
    #[test]
    fn internal_trees_are_not_subvolumes() {
        assert!(is_subvolume_id(FS_TREE_OBJECTID), "the default subvolume");
        assert!(is_subvolume_id(256), "the first created one");
        assert!(is_subvolume_id(1000));

        assert!(!is_subvolume_id(1), "the root tree");
        assert!(!is_subvolume_id(2), "the extent tree");
        assert!(!is_subvolume_id(255), "below the free range");

        // −9 as unsigned, which is a real tree on a real filesystem and
        // is what a lower bound alone lets through.
        assert!(
            !is_subvolume_id(u64::MAX - 8),
            "a negatively numbered internal tree is not a subvolume"
        );
        assert!(!is_subvolume_id(u64::MAX));
    }

    /// The name sits after a `dirid` and a `sequence`, and its length is
    /// declared rather than implied by the item's size.
    #[test]
    fn a_reference_name_is_read_from_its_declared_length() {
        let mut item = vec![0u8; 18];
        item[16..18].copy_from_slice(&3u16.to_le_bytes());
        item.extend_from_slice(b"sub");
        assert_eq!(reference_name(&item).as_deref(), Some(&b"sub"[..]));

        // A length that runs past the item is refused rather than
        // panicking or reading whatever follows.
        let mut lying = vec![0u8; 18];
        lying[16..18].copy_from_slice(&99u16.to_le_bytes());
        lying.extend_from_slice(b"sub");
        assert_eq!(reference_name(&lying), None);

        assert_eq!(
            reference_name(&[0u8; 4]),
            None,
            "too short to hold a header"
        );
    }

    /// A nested subvolume's path is its whole chain, and the default
    /// one's is the root.
    #[test]
    fn a_path_is_the_chain_of_parents() {
        let mut names = std::collections::BTreeMap::new();
        names.insert(256u64, (5u64, b"sub".to_vec()));
        names.insert(257u64, (256u64, b"inner".to_vec()));

        assert_eq!(path_of(5, &names), "/");
        assert_eq!(path_of(256, &names), "sub");
        assert_eq!(path_of(257, &names), "sub/inner");
    }

    /// References that form a cycle must not hang the listing — which is
    /// the tool someone reaches for when they already suspect the
    /// filesystem is damaged.
    #[test]
    fn a_cycle_in_the_references_terminates() {
        let mut names = std::collections::BTreeMap::new();
        names.insert(256u64, (257u64, b"a".to_vec()));
        names.insert(257u64, (256u64, b"b".to_vec()));

        // Any answer will do; not returning is the failure.
        let path = path_of(256, &names);
        assert!(!path.is_empty());
    }
}
