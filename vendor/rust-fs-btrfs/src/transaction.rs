//! Which blocks a change makes the filesystem rewrite.
//!
//! Copy-on-write means nothing is modified where it lies. Changing one
//! byte of one leaf means a new leaf somewhere else, a new node above it
//! pointing there, and so on to the root — then the root tree, which
//! names that tree's root and has therefore changed too. This works out
//! that set.
//!
//! # Why it is not just "walk up to the root"
//!
//! Because the walk does not stop at the root. `docs/cow-transaction.md`
//! measured a real transaction: a filesystem where a mount cycle changed
//! NOTHING still rewrote four blocks — the root tree, the extent tree,
//! the free-space tree and the dev tree — and swapped four
//! `METADATA_ITEM`s, four in and four out.
//!
//! That is the recursion, visible. Rewriting a block means allocating
//! one, allocating means recording it in the extent tree, and the extent
//! tree lives in blocks that must themselves be allocated. It terminates
//! because a copy-on-write rewrite is an allocation AND a release, so the
//! extent tree ends up recording its own new blocks rather than growing
//! without bound.
//!
//! # What this computes, and what it does not
//!
//! It computes the closure: given blocks whose contents changed, every
//! block that must be rewritten as a consequence, and a new address for
//! each. That is the part the measurement pinned down.
//!
//! It does not EDIT the extent tree. Adding and removing the
//! `METADATA_ITEM`s that record the plan means inserting into and
//! deleting from a leaf, with splits and merges when one fills or
//! empties, and none of that is implemented. So a plan says what a
//! transaction would cost and where everything would go; it does not yet
//! produce the item changes that make it true.
//!
//! That boundary is deliberate. The plan is checkable against what the
//! kernel actually did — see `tests/transaction_plan.rs` — and being
//! checkable before it is complete is worth more than being complete and
//! unverified.

use crate::chunk::objectid;
use crate::error::{Error, Result};
use crate::fs::Filesystem;
use std::collections::{BTreeMap, BTreeSet};

/// One block moving from where it is to where it will be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rewrite {
    /// Where it is now. Released once the transaction commits.
    pub old: u64,
    /// Where the new copy goes.
    pub new: u64,
    /// The tree it belongs to.
    pub owner: u64,
    /// Its height above the leaves.
    pub level: u8,
}

/// What a transaction will do.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    /// Every block that must be written, in no particular order — the
    /// commit sequencer writes them all before its barrier, so the order
    /// among them carries no meaning.
    pub rewrites: Vec<Rewrite>,
}

impl Plan {
    /// The addresses this frees.
    pub fn released(&self) -> Vec<u64> {
        self.rewrites.iter().map(|r| r.old).collect()
    }

    /// The addresses this takes.
    pub fn allocated(&self) -> Vec<u64> {
        self.rewrites.iter().map(|r| r.new).collect()
    }

    /// Which trees the transaction touches, in objectid order.
    pub fn trees(&self) -> BTreeSet<u64> {
        self.rewrites.iter().map(|r| r.owner).collect()
    }

    /// How much `bytes_used` moves.
    ///
    /// Zero for any plan that rewrites blocks without adding or removing
    /// any, which is every plan this produces — one release for every
    /// allocation. It is a method rather than a constant because that
    /// stops being true the moment a leaf split is implemented, and a
    /// caller should be asking rather than assuming.
    pub fn usage_delta(&self, nodesize: u64) -> i128 {
        (self.allocated().len() as i128 - self.released().len() as i128) * nodesize as i128
    }
}

/// One file extent a write replaces with a newly allocated copy (#61).
///
/// The copy holds exactly the bytes the file's item covers, so the new
/// item references the whole of it — offset zero, `num_bytes` equal to
/// the extent's length — whatever window of the old extent the old item
/// referenced. The old extent is released whole, which is right only
/// because its one reference is this item; the planner refuses anything
/// else before a `DataMove` is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DataMove {
    /// The extent being released.
    pub old: u64,
    /// Its length, as the extent tree's `EXTENT_ITEM` key records it.
    pub old_len: u64,
    /// The offset in the old extent's `EXTENT_DATA_REF`: the file offset
    /// at which the extent's first byte would sit, which is the item's
    /// key offset less its offset into the extent.
    pub old_ref_offset: u64,
    /// Where the copy goes.
    pub new: u64,
    /// The copy's length: the item's `num_bytes`.
    pub len: u64,
    /// The key offset of the file's `EXTENT_DATA` item.
    pub file_offset: u64,
}

/// The file-data half of a transaction: one file whose extents move.
///
/// Empty for a transaction that only relocates tree blocks, which is
/// what [`Filesystem::plan_transaction_closed`] and
/// [`Filesystem::render_plan`] produce.
#[derive(Debug, Clone, Default)]
pub(crate) struct DataWrite {
    /// The tree holding the file.
    pub root: u64,
    /// The file.
    pub ino: u64,
    /// Each extent replaced.
    pub moves: Vec<DataMove>,
    /// The modification time stamped on the inode, as seconds and
    /// nanoseconds since the epoch.
    pub time: (u64, u32),
}

impl DataWrite {
    /// Every address whose extent-tree record changes.
    fn addresses(&self) -> Vec<u64> {
        self.moves.iter().flat_map(|m| [m.old, m.new]).collect()
    }
}

/// What a tree walk hands to its visitor: a block's address, its bytes,
/// its level, and the node that points at it — `None` for a root.
/// What [`Filesystem::for_each_tree_block`] hands a visitor: the
/// block's address, the **parsed** block, and the address of the node
/// pointing at it (`None` for a root).
///
/// It used to pass `&[u8]`. The walk parses every block anyway, so
/// handing over the bytes meant each of the three visitors re-parsed
/// the leaf by hand — `25` as a bare literal in four places, the item
/// key at `+0..17`, the data offset at `+17..21` — all of which
/// `btree::Item` and `TreeBlock::item_data` already own, in the module
/// this file already imports from.
type BlockVisitor<'a> = &'a mut dyn FnMut(u64, &crate::btree::TreeBlock, Option<u64>);

/// A block's place in the tree it belongs to.
#[derive(Debug, Clone, Copy)]
struct Placement {
    parent: Option<u64>,
    owner: u64,
    level: u8,
}

impl Filesystem {
    /// Work out every block that must be rewritten if `dirty` change.
    ///
    /// The result includes `dirty` itself, every ancestor of each up to
    /// its tree's root, and the root tree's own path — because when a
    /// tree's root moves, the `ROOT_ITEM` naming it has changed, and the
    /// leaf holding that item is itself a block that must be rewritten.
    ///
    /// # Errors
    ///
    /// [`Error::UnsupportedFeature`] if a block in `dirty` is not part
    /// of any tree this can reach, which means it is not a tree block or
    /// is not live — planning around it would allocate for something
    /// nothing points at.
    ///
    /// Propagates an allocation failure when there is nowhere to put the
    /// new copies.
    pub fn plan_transaction(&self, dirty: &[u64]) -> Result<Plan> {
        let places = self.placements()?;

        // The closure: everything dirty, plus every ancestor, plus the
        // root tree's path to whichever ROOT_ITEM leaf named a root that
        // moved.
        let mut touched: BTreeSet<u64> = BTreeSet::new();
        let mut queue: Vec<u64> = dirty.to_vec();

        while let Some(at) = queue.pop() {
            if !touched.insert(at) {
                continue;
            }
            let place = places.get(&at).ok_or_else(|| {
                Error::UnsupportedFeature(format!(
                    "the block at {at} is not reachable from any tree, so there is nothing \
                     above it to rewrite"
                ))
            })?;

            match place.parent {
                // Not a root: its parent points at it and must change.
                Some(parent) => queue.push(parent),
                // A tree's root. If it is not the ROOT TREE's own root,
                // the root tree holds a ROOT_ITEM naming it, and that
                // leaf has changed.
                None if place.owner != objectid::ROOT_TREE => {
                    if let Some(leaf) = self.root_item_leaf(place.owner)? {
                        queue.push(leaf);
                    }
                }
                None => {}
            }
        }

        // A new home for each, all distinct and none of them somewhere
        // already in use.
        let mut plan = Plan::default();
        let mut taken: BTreeSet<u64> = BTreeSet::new();
        for old in touched {
            let place = places[&old];
            let new = self.next_free_block(&taken)?;
            taken.insert(new);
            plan.rewrites.push(Rewrite {
                old,
                new,
                owner: place.owner,
                level: place.level,
            });
        }
        Ok(plan)
    }

    /// Somewhere to put a block that is not already spoken for.
    ///
    /// [`Filesystem::find_metadata_block`] does not record what it hands
    /// out, so asking twice gives the same answer twice. Until a
    /// transaction records its allocations, the addresses it has already
    /// chosen are held here.
    fn next_free_block(&self, taken: &BTreeSet<u64>) -> Result<u64> {
        let nodesize = self.sb.nodesize as u64;
        let groups: Vec<_> = self
            .block_groups()?
            .into_iter()
            .filter(|g| g.holds_metadata())
            .collect();

        for runs in self.free_extents_by_group(&groups)? {
            for run in runs {
                let mut at = run.start.next_multiple_of(nodesize);
                while at + nodesize <= run.end() {
                    if !taken.contains(&at) && !self.on_superblock_copy(at, nodesize)? {
                        return Ok(at);
                    }
                    at += nodesize;
                }
            }
        }
        Err(Error::UnsupportedFeature(format!(
            "no metadata block group has room for another {nodesize}-byte block; \
             allocating a new block group is not implemented"
        )))
    }

    /// The root block a `ROOT_ITEM` names, or `None` if its body does
    /// not reach that far.
    ///
    /// `None` is a *refusal to guess*, not a "no root": the item claims
    /// a body that the block does not contain, which means the leaf is
    /// malformed. The caller skips it, and the tree it would have named
    /// goes undiscovered — which surfaces later as "not reachable from
    /// any tree" rather than as a parse error. That indirection is
    /// pre-existing; what is new is that the bounds decision is made in
    /// one place, by `TreeBlock::item_data`, instead of by an inline
    /// `if off + BYTENR + 8 <= block.len()` the reader has to
    /// reconstruct.
    fn root_item_bytenr(block: &crate::btree::TreeBlock, item: &crate::btree::Item) -> Option<u64> {
        let data = block.item_data(item)?;
        let field = data.get(root_item::BYTENR..root_item::BYTENR + 8)?;
        Some(u64::from_le_bytes(field.try_into().ok()?))
    }

    /// The root tree leaf holding the `ROOT_ITEM` for `objectid`.
    fn root_item_leaf(&self, objectid: u64) -> Result<Option<u64>> {
        let mut found = None;
        self.for_each_tree_block(self.sb.root, &mut |at, block, _| {
            if found.is_some() {
                return;
            }
            let Some(items) = block.body.items() else {
                return;
            };
            if items
                .iter()
                .any(|it| it.key.objectid == objectid && it.key.key_type == ROOT_ITEM_KEY)
            {
                found = Some(at);
            }
        })?;
        Ok(found)
    }

    /// Where every reachable block sits: its parent, tree and level.
    fn placements(&self) -> Result<BTreeMap<u64, Placement>> {
        let mut out = BTreeMap::new();
        let mut roots = vec![self.sb.root];

        while let Some(root) = roots.pop() {
            if root == 0 {
                continue;
            }
            let mut found_roots = Vec::new();
            self.for_each_tree_block(root, &mut |at, block, parent| {
                let owner = block.header.owner;
                let level = block.header.level;
                out.insert(
                    at,
                    Placement {
                        parent,
                        owner,
                        level,
                    },
                );

                if owner != objectid::ROOT_TREE {
                    return;
                }
                // A root tree leaf: each ROOT_ITEM names another tree.
                let Some(items) = block.body.items() else {
                    return;
                };
                for item in items {
                    if item.key.key_type != ROOT_ITEM_KEY {
                        continue;
                    }
                    let Some(b) = Self::root_item_bytenr(block, item) else {
                        continue;
                    };
                    if b != 0 && !out.contains_key(&b) {
                        found_roots.push(b);
                    }
                }
            })?;
            roots.extend(found_roots);
        }
        Ok(out)
    }

    /// Walk a tree, handing each block to `visit` with its parent.
    ///
    /// `visit` receives the block's address, the parsed block, and the
    /// address of the node that points at it — `None` for a root. The
    /// walk has already parsed it to find the child pointers, so
    /// handing over the parse costs nothing and saves every visitor
    /// from redoing it by hand.
    ///
    /// # Errors
    ///
    /// Propagates a read failure. A block that will not read is skipped
    /// rather than fatal: a tree this driver cannot fully walk is still
    /// one whose reachable part is worth knowing.
    fn for_each_tree_block(&self, root: u64, visit: BlockVisitor) -> Result<()> {
        let reader = self.pool_reader();
        let tree = reader.tree();

        let mut stack = vec![(root, None)];
        let mut seen = BTreeSet::new();
        while let Some((at, parent)) = stack.pop() {
            if at == 0 || !seen.insert(at) {
                continue;
            }
            let Ok(block) = tree.read_block(at) else {
                continue;
            };
            visit(at, &block, parent);
            if let Some(ptrs) = block.body.key_ptrs() {
                for p in ptrs {
                    stack.push((p.blockptr, Some(at)));
                }
            }
        }
        Ok(())
    }
}

use crate::fs::{root_item, ROOT_ITEM_KEY};

impl Filesystem {
    /// Turn a plan into the blocks it says to write.
    ///
    /// Every block keeps its contents and changes its address. What that
    /// means depends on what the block is:
    ///
    /// - a **leaf** keeps its items exactly, and is re-stamped with its
    ///   new address and the new generation;
    /// - a **node** keeps its keys, but every child that also moved is
    ///   pointed at its new address — a node still naming the old one is
    ///   a tree that reads the version before the change;
    /// - a **root tree leaf** additionally has its `ROOT_ITEM`s
    ///   rewritten, because those name the roots of other trees, and a
    ///   tree whose root moved has a stale `ROOT_ITEM` otherwise.
    ///
    /// The result goes straight to [`Filesystem::commit`].
    ///
    /// # What it also does
    ///
    /// Two of the blocks it renders have their CONTENTS changed as well
    /// as their address, because they are the record of the move:
    ///
    /// - an **extent tree** leaf loses the `METADATA_ITEM` naming each
    ///   block's old address and gains one naming the new;
    /// - a **free-space tree** leaf has the affected block groups' free
    ///   runs recomputed, since that tree is the complement of the
    ///   extent tree;
    /// - an **extent tree** leaf holding a `BLOCK_GROUP_ITEM` has that
    ///   group's `used` moved by what the plan allocates in it less what
    ///   it releases there. The two ends of a move are often in different
    ///   groups, so `bytes_used` can add up while every group's count is
    ///   wrong (#223).
    ///
    /// This section previously said the opposite — that a relocation
    /// moves what is already there and records nothing — which was true
    /// when it was written and stopped being true seventy lines below,
    /// where `apply_records` was added. Left uncorrected it pointed a
    /// reader at exactly the bug the code exists to prevent.
    ///
    /// # Errors
    ///
    /// Propagates a read failure, and [`Error::UnsupportedFeature`] if a
    /// block cannot be re-encoded at its new size — which cannot happen
    /// for a straight relocation and is reported rather than assumed
    /// away — or if the plan touches a block group whose free-space
    /// records span more than one leaf (#177).
    pub fn render_plan(
        &self,
        plan: &Plan,
        generation: u64,
    ) -> Result<Vec<crate::commit::PlacedBlock>> {
        self.render_plan_with(plan, &DataWrite::default(), generation)
    }

    /// [`Filesystem::render_plan`] for a transaction that also moves a
    /// file's data extents.
    ///
    /// Besides what a relocation renders, the leaves holding the file's
    /// `EXTENT_DATA` items are pointed at the new extents and its inode
    /// item is stamped with the transaction; the extent tree drops each
    /// old data extent's record and gains one for each new; the
    /// free-space tree and the block groups' `used` count the data
    /// extents as well as the tree blocks.
    ///
    /// # Errors
    ///
    /// As [`Filesystem::render_plan`], and [`Error::UnsupportedFeature`]
    /// when an item the write changes is not in any leaf the plan
    /// rewrites, or is not the shape the planner saw.
    pub(crate) fn render_plan_with(
        &self,
        plan: &Plan,
        data: &DataWrite,
        generation: u64,
    ) -> Result<Vec<crate::commit::PlacedBlock>> {
        use crate::commit::PlacedBlock;
        use crate::leaf_edit::OwnedItem;
        use crate::tree_write::{build_leaf, build_node, chunk_tree_uuid_of, BlockIdentity};

        // Where each moved block is going.
        // Before any block is rendered, so the answer does not depend on
        // which leaf the loop below happens to reach first.
        if plan.trees().contains(&objectid::FREE_SPACE_TREE) {
            self.refuse_free_space_straddle(plan, data)?;
        }

        let moved: BTreeMap<u64, u64> = plan.rewrites.iter().map(|r| (r.old, r.new)).collect();

        // How far each block group's `used` moves, and which of them
        // have had it written so far (#223).
        let deltas = self.block_group_deltas(plan, data)?;
        let mut used_written: BTreeSet<u64> = BTreeSet::new();
        // How much of the file write has been applied, so a part that
        // landed in no rewritten leaf is an error rather than a loss.
        let mut applied = DataApplied::default();

        let mut out = Vec::with_capacity(plan.rewrites.len());
        for rewrite in &plan.rewrites {
            let block = self.read_tree_block(rewrite.old)?;
            let raw = block.bytes().to_vec();
            let id = BlockIdentity {
                bytenr: rewrite.new,
                owner: rewrite.owner,
                generation,
                level: rewrite.level,
                flags: crate::tree_write::flags_for_new_block(),
                chunk_tree_uuid: chunk_tree_uuid_of(&raw),
            };

            let bytes = match block.body.key_ptrs() {
                Some(ptrs) => {
                    // A node: follow every child that moved.
                    let updated: Vec<_> = ptrs
                        .iter()
                        .map(|p| {
                            let mut p = *p;
                            if let Some(&to) = moved.get(&p.blockptr) {
                                p.blockptr = to;
                                p.generation = generation;
                            }
                            p
                        })
                        .collect();
                    build_node(&self.sb, id, &updated)?
                }
                None => {
                    let items = block.body.items().unwrap_or(&[]);
                    let mut owned: Vec<OwnedItem> = items
                        .iter()
                        .filter_map(|item| {
                            block.item_data(item).map(|data| OwnedItem {
                                key: item.key,
                                data: data.to_vec(),
                            })
                        })
                        .collect();

                    // An extent tree leaf carries the record of what is
                    // allocated, so a relocation changes its CONTENTS as
                    // well as its address: every block the plan moves
                    // loses the item naming where it was and gains one
                    // naming where it went. Without this the tree is
                    // correct and the extent tree describes a filesystem
                    // that no longer exists.
                    if rewrite.owner == objectid::EXTENT_TREE {
                        owned = self.apply_records(
                            rewrite.old,
                            owned,
                            plan,
                            data,
                            generation,
                            &mut applied,
                        )?;
                        apply_block_group_used(&mut owned, &deltas, &mut used_written)?;
                    }

                    // The free-space tree is the complement of the
                    // extent tree, so a transaction changes both. A leaf
                    // left saying where things used to be is what `btrfs
                    // check` calls "cache appears valid but isn't".
                    if rewrite.owner == objectid::FREE_SPACE_TREE {
                        owned = self.apply_free_space(owned, plan, data)?;
                    }

                    // The file's own items: its extents now live elsewhere.
                    if !data.moves.is_empty() && rewrite.owner == data.root {
                        apply_file_write(&mut owned, data, generation, &mut applied)?;
                    }

                    // A root tree leaf names other trees' roots.
                    if rewrite.owner == objectid::ROOT_TREE {
                        for item in &mut owned {
                            if item.key.key_type != ROOT_ITEM_KEY
                                || item.data.len() < root_item::LEVEL + 1
                            {
                                continue;
                            }
                            let at = u64::from_le_bytes(
                                item.data[root_item::BYTENR..root_item::BYTENR + 8]
                                    .try_into()
                                    .expect("8 bytes"),
                            );
                            if let Some(&to) = moved.get(&at) {
                                item.data[root_item::BYTENR..root_item::BYTENR + 8]
                                    .copy_from_slice(&to.to_le_bytes());
                                item.data[root_item::GENERATION..root_item::GENERATION + 8]
                                    .copy_from_slice(&generation.to_le_bytes());
                                // An item long enough to carry the second
                                // copy must carry the same value, or the
                                // kernel discards the root's newer fields.
                                if let Some(v2) = item
                                    .data
                                    .get_mut(root_item::GENERATION_V2..root_item::GENERATION_V2 + 8)
                                {
                                    v2.copy_from_slice(&generation.to_le_bytes());
                                }
                            }
                        }
                    }

                    let borrowed: Vec<_> = owned.iter().map(|i| i.as_leaf_item()).collect();
                    build_leaf(&self.sb, id, &borrowed)?
                }
            };

            out.push(PlacedBlock {
                logical: rewrite.new,
                bytes,
            });
        }

        // NOT A SKIP. A group whose `used` moves but whose item was in no
        // leaf the plan rewrites keeps the old count, and `btrfs check`
        // then reports "block group [...] used X but extent items used
        // Y". A plan closed by `plan_transaction_closed` always holds
        // that leaf; one built another way may not.
        if let Some((start, delta)) = deltas.iter().find(|(g, _)| !used_written.contains(g)) {
            return Err(Error::UnsupportedFeature(format!(
                "the plan moves the used count of the block group at {start} by {delta} bytes, \
                 but the leaf holding its block group item is not one the plan rewrites"
            )));
        }
        // NOR IS A FILE WRITE. Every part of it is in a leaf the plan was
        // closed over, so a part not applied means the leaf it is in was
        // missed — and committing without it leaves an extent nothing
        // references, or an item naming one nothing records.
        let n = data.moves.len();
        let inode = usize::from(n > 0);
        if applied.items != n || applied.inodes != inode {
            return Err(Error::UnsupportedFeature(format!(
                "the write moves {n} extents of inode {}, and the plan rewrote {} of its \
                 extent items and {} of its inode items",
                data.ino, applied.items, applied.inodes
            )));
        }
        if applied.released != n || applied.recorded != n {
            return Err(Error::UnsupportedFeature(format!(
                "the write moves {n} data extents, and the plan released the records of {} and \
                 recorded {}",
                applied.released, applied.recorded
            )));
        }
        Ok(out)
    }

    /// Where the root tree ends up, for a plan that moves it.
    ///
    /// The superblock names the root tree, so committing a plan needs
    /// this. Returns `None` when the plan does not move the root tree,
    /// which means the superblock's existing value still stands.
    pub fn planned_root(&self, plan: &Plan) -> Option<u64> {
        plan.rewrites
            .iter()
            .find(|r| r.old == self.sb.root)
            .map(|r| r.new)
    }
}

impl Filesystem {
    /// Plan a transaction including the extent tree's own rewrite.
    ///
    /// [`Filesystem::plan_transaction`] computes the spine: the dirty
    /// blocks, their ancestors, and the root tree leaf naming a tree
    /// whose root moved. That is not a whole transaction, because moving
    /// a block means recording the move — a `METADATA_ITEM` added for
    /// the new address and the old one's removed — and those items live
    /// in extent tree leaves, which are themselves blocks that must then
    /// be rewritten.
    ///
    /// That is the recursion `docs/cow-transaction.md` measured, where a
    /// commit changing NOTHING still rewrote four blocks. It terminates
    /// because the extent tree ends up recording its own new blocks
    /// rather than growing: each rewrite is an allocation and a release.
    ///
    /// This closes it by iterating. Each round works out which extent
    /// tree leaves hold the records for the blocks moved so far, adds
    /// them to the dirty set, and plans again. When a round adds nothing
    /// the plan is closed.
    ///
    /// # Errors
    ///
    /// As [`Filesystem::plan_transaction`], and
    /// [`Error::UnsupportedFeature`] if the iteration does not settle
    /// within `rounds`. That is reported rather than papered over: a
    /// transaction whose own bookkeeping keeps producing more work is
    /// one that needs the kernel's reservation machinery, not another
    /// turn of the loop.
    pub fn plan_transaction_closed(&self, dirty: &[u64], rounds: usize) -> Result<Plan> {
        self.plan_transaction_closed_with(dirty, &DataWrite::default(), rounds)
    }

    /// [`Filesystem::plan_transaction_closed`], closed over a file
    /// write's data extents as well: the extent tree leaves holding their
    /// records, the free-space tree leaves describing their groups, and
    /// the leaves of those groups' `BLOCK_GROUP_ITEM`s are dirty too.
    pub(crate) fn plan_transaction_closed_with(
        &self,
        dirty: &[u64],
        data: &DataWrite,
        rounds: usize,
    ) -> Result<Plan> {
        let mut seed: BTreeSet<u64> = dirty.iter().copied().collect();

        for _ in 0..rounds {
            let list: Vec<u64> = seed.iter().copied().collect();
            let plan = self.plan_transaction(&list)?;

            // Every address whose record changes: the old ones lose a
            // METADATA_ITEM, the new ones gain one -- and each data
            // extent the write moves loses or gains an EXTENT_ITEM.
            let mut touched: Vec<u64> = plan.released();
            touched.extend(plan.allocated());
            touched.extend(data.addresses());

            let before = seed.len();
            seed.extend(self.extent_leaves_for(&touched)?);
            // The free-space tree tracks the same moves from the other
            // side, so its leaves for those addresses are dirty too.
            seed.extend(self.free_space_leaves_for(&touched)?);
            // A group the plan takes more from than it gives back, or the
            // reverse, has its BLOCK_GROUP_ITEM's `used` rewritten, so
            // the leaf holding that item is dirty too (#223).
            let groups: BTreeSet<u64> = self.block_group_deltas(&plan, data)?.into_keys().collect();
            seed.extend(self.block_group_item_leaves(&groups)?.into_values());
            if seed.len() == before {
                return Ok(plan);
            }
        }

        Err(Error::UnsupportedFeature(format!(
            "the transaction did not settle in {rounds} rounds: recording each round's \
             moves keeps dirtying extent tree leaves that were not already in it. \
             Breaking that needs reservations rather than another iteration"
        )))
    }

    /// The extent tree leaves that hold, or would hold, the records for
    /// `addresses`.
    ///
    /// "Would hold" is the important half. An insert dirties the leaf it
    /// lands in even though nothing is filed under that key yet, and
    /// which leaf that is follows the same rule a descent uses: the LAST
    /// leaf whose first key is not greater than the one being inserted.
    ///
    /// A range test — is the address between this leaf's first and last
    /// key — is not that rule, and gets the common case wrong. A newly
    /// allocated address is usually PAST every key in the tree, so it
    /// falls in no leaf's range, no leaf is dirtied, and the record is
    /// never written. The block then has no back reference and `btrfs
    /// check` says so: "tree extent[...] has no backref item in extent
    /// tree". It only showed up on a fixture whose free space happened
    /// to lie beyond the last record rather than among them.
    fn extent_leaves_for(&self, addresses: &[u64]) -> Result<BTreeSet<u64>> {
        let root = self.tree_root(objectid::EXTENT_TREE)?;
        self.leaves_holding(root, addresses)
    }

    /// The leaves of `root` that an insert of each address would land
    /// in.
    fn leaves_holding(&self, root: u64, addresses: &[u64]) -> Result<BTreeSet<u64>> {
        if addresses.is_empty() {
            return Ok(BTreeSet::new());
        }

        // Every leaf, by its first key.
        let mut leaves: Vec<(u64, u64)> = Vec::new();
        self.for_each_tree_block(root, &mut |at, block, _| {
            let Some(items) = block.body.items() else {
                return;
            };
            if let Some(first) = items.first() {
                leaves.push((first.key.objectid, at));
            }
        })?;
        if leaves.is_empty() {
            return Ok(BTreeSet::new());
        }
        leaves.sort();

        let mut out = BTreeSet::new();
        for a in addresses {
            // The last leaf that begins at or before this key, or the
            // first leaf when the key precedes everything.
            let idx = match leaves.binary_search_by(|(first, _)| first.cmp(a)) {
                Ok(i) => i,
                Err(0) => 0,
                Err(i) => i - 1,
            };
            out.insert(leaves[idx].1);
        }
        Ok(out)
    }
}

impl Filesystem {
    /// Apply a plan's allocations and releases to one extent tree leaf.
    ///
    /// Each moved block loses the `METADATA_ITEM` naming its old address
    /// and gains one naming the new. Only the items belonging in THIS
    /// leaf are touched: which leaf an address belongs to is decided by
    /// the key range, and every leaf that any of them falls into is in
    /// the plan — that is what closing the plan over its own bookkeeping
    /// guarantees.
    ///
    /// # Errors
    ///
    /// Propagates a refusal from the leaf editor. An address whose
    /// record is missing is an error rather than a skip: it means the
    /// extent tree does not say what the plan believes, and carrying on
    /// would leave a block recorded as allocated for ever.
    fn apply_records(
        &self,
        leaf: u64,
        items: Vec<crate::leaf_edit::OwnedItem>,
        plan: &Plan,
        data: &DataWrite,
        generation: u64,
        applied: &mut DataApplied,
    ) -> Result<Vec<crate::leaf_edit::OwnedItem>> {
        use crate::extent_write::{record_tree_block, TreeBlockAllocation};
        use crate::leaf_edit::{delete, insert, OwnedItem};

        // Which leaf each address belongs to, by the rule a descent
        // uses — NOT by whether this leaf's existing keys bracket it. A
        // newly allocated address is usually past every key in the tree,
        // and a bracket test skips it: the record is never written and
        // `btrfs check` reports the block as having no backref item.
        let root = self.tree_root(objectid::EXTENT_TREE)?;
        let mine =
            |at: u64| -> Result<bool> { Ok(self.leaves_holding(root, &[at])?.contains(&leaf)) };

        let mut out = items;

        // Releases first, so the leaf is at its smallest before
        // anything is added to it.
        for rewrite in &plan.rewrites {
            if !mine(rewrite.old)? {
                continue;
            }
            let key = TreeBlockAllocation {
                bytenr: rewrite.old,
                level: rewrite.level,
                generation,
                owner: rewrite.owner,
            }
            .key();
            // NOT GUARDED. A missing record is the error `delete` reports:
            // the extent tree does not say what the plan believes, and
            // skipping it left the old block recorded as allocated for
            // ever -- on a volume without SKINNY_METADATA, where the record
            // is an EXTENT_ITEM under another key, on every transaction
            // (#87).
            // ONE REFERENCE, OR NOT OURS TO RELEASE (#178). Deleting the
            // record is how a block with a single owner is freed. A block
            // with more is shared, usually by a snapshot of a tree whose
            // root is a node, and the other tree still points at it once
            // this one has moved on. Releasing it means dropping one
            // reference, and giving the block's children references of
            // their own, which is not implemented.
            if let Some(item) = out.iter().find(|i| i.key == key) {
                let refs = item
                    .data
                    .get(..8)
                    .map(|b| u64::from_le_bytes(b.try_into().expect("8 bytes")));
                if refs != Some(1) {
                    return Err(Error::UnsupportedFeature(format!(
                        "the tree block at {} has {} references, so another tree -- most \
                         likely a snapshot -- still points at it; moving it would free a \
                         block that tree reads, and dropping one reference is not \
                         implemented",
                        rewrite.old,
                        refs.map_or("an unreadable number of".to_string(), |r| r.to_string())
                    )));
                }
            }
            out = delete(&out, &key)?;
        }

        // Each data extent the write releases: its record goes, but only
        // once it is shown to be the record the planner refused
        // everything else for -- one reference, held inline by this very
        // file item. Anything else would free bytes another file or a
        // snapshot still reads.
        for m in &data.moves {
            if !mine(m.old)? {
                continue;
            }
            let key = DiskKey {
                objectid: m.old,
                key_type: key_type::EXTENT_ITEM,
                offset: m.old_len,
            };
            let body = out.iter().find(|i| i.key == key).map(|i| i.data.as_slice());
            let expected = data_extent_body(1, None, data.root, data.ino, m.old_ref_offset);
            let held = body.is_some_and(|b| {
                b.len() == expected.len()
                    && b[..8] == expected[..8]
                    && b[extent_body::FLAGS..] == expected[extent_body::FLAGS..]
            });
            if !held {
                return Err(Error::UnsupportedFeature(format!(
                    "the data extent at {} is not recorded as one reference held inline by \
                     inode {} at {} in tree {}, so releasing it could free bytes something \
                     else reads",
                    m.old, data.ino, m.old_ref_offset, data.root
                )));
            }
            out = delete(&out, &key)?;
            applied.released += 1;
        }

        for rewrite in &plan.rewrites {
            if !mine(rewrite.new)? {
                continue;
            }
            let alloc = TreeBlockAllocation {
                bytenr: rewrite.new,
                level: rewrite.level,
                generation,
                owner: rewrite.owner,
            };
            let (key, body) = record_tree_block(&self.sb, alloc)?;
            // Nor here: a record already under the new address means the
            // free-space picture that chose it was wrong, and `insert`
            // refuses a duplicate key.
            out = insert(
                self.sb.nodesize,
                &out,
                OwnedItem {
                    key,
                    data: body.to_vec(),
                },
            )?;
        }

        // And each copy the write allocated: one reference, inline, from
        // the file item that now names it at offset zero.
        for m in &data.moves {
            if !mine(m.new)? {
                continue;
            }
            out = insert(
                self.sb.nodesize,
                &out,
                OwnedItem {
                    key: DiskKey {
                        objectid: m.new,
                        key_type: key_type::EXTENT_ITEM,
                        offset: m.len,
                    },
                    data: data_extent_body(1, Some(generation), data.root, data.ino, m.file_offset)
                        .to_vec(),
                },
            )?;
            applied.recorded += 1;
        }
        Ok(out)
    }
}

/// How much of a [`DataWrite`] the rendered leaves carried.
#[derive(Debug, Default)]
struct DataApplied {
    /// `EXTENT_DATA` items pointed at their new extent.
    items: usize,
    /// Inode items stamped.
    inodes: usize,
    /// Old data extents whose record was removed.
    released: usize,
    /// New data extents recorded.
    recorded: usize,
}

/// Offsets within a data extent's `EXTENT_ITEM` body carrying one inline
/// `EXTENT_DATA_REF` — the 53-byte shape `docs/transaction-format.md`
/// measured.
mod extent_body {
    /// `u64` reference count.
    pub const REFS: usize = 0;
    /// `u64` the transaction that allocated the extent.
    pub const GENERATION: usize = 8;
    /// `u64` `EXTENT_FLAG_DATA`.
    pub const FLAGS: usize = 16;
    /// `u8` the inline reference's type, `EXTENT_DATA_REF`.
    pub const REF_TYPE: usize = 24;
    /// `u64` the tree holding the referencing item.
    pub const REF_ROOT: usize = 25;
    /// `u64` the inode.
    pub const REF_OBJECTID: usize = 33;
    /// `u64` the file offset of the extent's first byte.
    pub const REF_OFFSET: usize = 41;
    /// `u32` how many of that inode's items reference the extent there.
    pub const REF_COUNT: usize = 49;
    /// The whole item.
    pub const SIZE: usize = 53;
}

/// `BTRFS_EXTENT_DATA_REF_KEY`, as an inline reference type.
const EXTENT_DATA_REF: u8 = 178;

/// A data extent's `EXTENT_ITEM` body with one inline `EXTENT_DATA_REF`
/// of count one. The generation is left zero when `None`, for a caller
/// comparing everything else.
fn data_extent_body(
    refs: u64,
    generation: Option<u64>,
    root: u64,
    ino: u64,
    offset: u64,
) -> [u8; extent_body::SIZE] {
    use crate::extent_write::EXTENT_FLAG_DATA;
    use extent_body::*;
    let mut out = [0u8; SIZE];
    out[REFS..REFS + 8].copy_from_slice(&refs.to_le_bytes());
    out[GENERATION..GENERATION + 8].copy_from_slice(&generation.unwrap_or(0).to_le_bytes());
    out[FLAGS..FLAGS + 8].copy_from_slice(&EXTENT_FLAG_DATA.to_le_bytes());
    out[REF_TYPE] = EXTENT_DATA_REF;
    out[REF_ROOT..REF_ROOT + 8].copy_from_slice(&root.to_le_bytes());
    out[REF_OBJECTID..REF_OBJECTID + 8].copy_from_slice(&ino.to_le_bytes());
    out[REF_OFFSET..REF_OFFSET + 8].copy_from_slice(&offset.to_le_bytes());
    out[REF_COUNT..REF_COUNT + 4].copy_from_slice(&1u32.to_le_bytes());
    out
}

/// Point the file's `EXTENT_DATA` items at their new extents and stamp
/// its inode item, in one fs tree leaf.
///
/// Every field is rewritten in place: an item keeps its size, so the
/// leaf cannot overflow. Each item is checked against what the planner
/// saw before it is changed — a regular, uncompressed extent at the old
/// address covering exactly the moved length — because a leaf that says
/// something else is not the file the plan was made for.
fn apply_file_write(
    items: &mut [crate::leaf_edit::OwnedItem],
    data: &DataWrite,
    generation: u64,
    applied: &mut DataApplied,
) -> Result<()> {
    use crate::fs::file_extent as fe;
    use crate::inode::{offsets as io, INODE_ITEM_KEY, INODE_ITEM_SIZE};
    let le64 = |b: &[u8], at: usize| u64::from_le_bytes(b[at..at + 8].try_into().expect("8"));
    let put64 = |b: &mut [u8], at: usize, v: u64| b[at..at + 8].copy_from_slice(&v.to_le_bytes());

    for item in items.iter_mut() {
        if item.key.objectid != data.ino {
            continue;
        }
        let d = &mut item.data;
        match item.key.key_type {
            INODE_ITEM_KEY => {
                if d.len() < INODE_ITEM_SIZE {
                    return Err(Error::UnsupportedFeature(format!(
                        "inode {}'s item is {} bytes, shorter than an inode",
                        data.ino,
                        d.len()
                    )));
                }
                // What the kernel changes when it writes a file's data:
                // the transaction that last touched it, its change
                // counter, and its change and modification times.
                put64(d, io::TRANSID, generation);
                let sequence = le64(d, io::SEQUENCE).wrapping_add(1);
                put64(d, io::SEQUENCE, sequence);
                for at in [io::CTIME, io::MTIME] {
                    put64(d, at, data.time.0);
                    d[at + 8..at + 12].copy_from_slice(&data.time.1.to_le_bytes());
                }
                applied.inodes += 1;
            }
            EXTENT_DATA_KEY => {
                let Some(m) = data.moves.iter().find(|m| m.file_offset == item.key.offset) else {
                    continue;
                };
                let regular = d.len() == fe::REGULAR_SIZE
                    && d[fe::TYPE] == EXTENT_REGULAR
                    && d[fe::COMPRESSION] == 0
                    && d[fe::ENCRYPTION] == 0
                    && d[fe::OTHER_ENCODING..fe::OTHER_ENCODING + 2] == [0, 0];
                if !regular
                    || le64(d, fe::DISK_BYTENR) != m.old
                    || le64(d, fe::NUM_BYTES) != m.len
                    || item.key.offset.checked_sub(le64(d, fe::OFFSET)) != Some(m.old_ref_offset)
                {
                    return Err(Error::UnsupportedFeature(format!(
                        "inode {}'s extent item at {} is not the regular extent of {} bytes at \
                         {} the write was planned against",
                        data.ino, item.key.offset, m.len, m.old
                    )));
                }
                put64(d, fe::GENERATION, generation);
                put64(d, fe::RAM_BYTES, m.len);
                put64(d, fe::DISK_BYTENR, m.new);
                put64(d, fe::DISK_NUM_BYTES, m.len);
                put64(d, fe::OFFSET, 0);
                applied.items += 1;
            }
            _ => {}
        }
    }
    Ok(())
}

use crate::chunk::{key_type, DiskKey};
use crate::fs::{EXTENT_DATA_KEY, EXTENT_REGULAR};

impl Filesystem {
    /// How far each block group's `used` moves under `plan`, in bytes:
    /// a node for every block allocated in it, less one for every block
    /// released from it. Groups that come out even are left out.
    ///
    /// # Errors
    ///
    /// [`Error::UnsupportedFeature`] for an address no block group
    /// holds, which a plan cannot have chosen and a live tree block
    /// cannot be at.
    fn block_group_deltas(&self, plan: &Plan, data: &DataWrite) -> Result<BTreeMap<u64, i128>> {
        let groups = self.block_groups()?;
        let group_of = |at: u64| {
            groups
                .iter()
                .find(|g| g.contains(at))
                .map(|g| g.start)
                .ok_or_else(|| {
                    Error::UnsupportedFeature(format!(
                        "the block at {at} is in no block group, so there is no used count \
                         to move for it"
                    ))
                })
        };
        let nodesize = i128::from(self.sb.nodesize);
        let mut out: BTreeMap<u64, i128> = BTreeMap::new();
        for rewrite in &plan.rewrites {
            *out.entry(group_of(rewrite.new)?).or_default() += nodesize;
            *out.entry(group_of(rewrite.old)?).or_default() -= nodesize;
        }
        // A data extent counts its own length, not a node's.
        for m in &data.moves {
            *out.entry(group_of(m.new)?).or_default() += i128::from(m.len);
            *out.entry(group_of(m.old)?).or_default() -= i128::from(m.old_len);
        }
        out.retain(|_, delta| *delta != 0);
        Ok(out)
    }

    /// The extent tree leaf holding the `BLOCK_GROUP_ITEM` of each group
    /// in `starts`, by group start.
    ///
    /// Found by the item itself rather than by the insert rule
    /// `leaves_holding` uses: the item exists, and the leaf that holds it
    /// is the one whose bytes change.
    fn block_group_item_leaves(&self, starts: &BTreeSet<u64>) -> Result<BTreeMap<u64, u64>> {
        let mut out = BTreeMap::new();
        if starts.is_empty() {
            return Ok(out);
        }
        let root = self.tree_root(objectid::EXTENT_TREE)?;
        self.for_each_tree_block(root, &mut |at, block, _| {
            let Some(items) = block.body.items() else {
                return;
            };
            for item in items {
                if item.key.key_type == BLOCK_GROUP_ITEM_KEY && starts.contains(&item.key.objectid)
                {
                    out.insert(item.key.objectid, at);
                }
            }
        })?;
        Ok(out)
    }
}

/// Move the `used` of every `BLOCK_GROUP_ITEM` in `items` that `deltas`
/// names, and note each group written in `written`.
///
/// # Errors
///
/// [`Error::UnsupportedFeature`] for an item too short to hold `used`, or
/// a count the move would take below zero or past the group's length —
/// either means the extent tree does not say what the plan believes.
fn apply_block_group_used(
    items: &mut [crate::leaf_edit::OwnedItem],
    deltas: &BTreeMap<u64, i128>,
    written: &mut BTreeSet<u64>,
) -> Result<()> {
    use crate::block_group::block_group_item::{SIZE, USED};
    for item in items.iter_mut() {
        if item.key.key_type != BLOCK_GROUP_ITEM_KEY {
            continue;
        }
        let Some(&delta) = deltas.get(&item.key.objectid) else {
            continue;
        };
        let start = item.key.objectid;
        let whole = item.data.len() >= SIZE;
        let field = item.data.get_mut(USED..USED + 8).filter(|_| whole);
        let Some(field) = field else {
            return Err(Error::UnsupportedFeature(format!(
                "the block group item at {start} is shorter than the structure it declares"
            )));
        };
        let used = u64::from_le_bytes((&*field).try_into().expect("8 bytes"));
        let moved = i128::from(used) + delta;
        if moved < 0 || moved > i128::from(item.key.offset) {
            return Err(Error::UnsupportedFeature(format!(
                "the block group at {start} records {used} bytes used, and the plan would move \
                 that by {delta} to outside its {} bytes",
                item.key.offset
            )));
        }
        field.copy_from_slice(&(moved as u64).to_le_bytes());
        written.insert(start);
    }
    Ok(())
}

use crate::chunk::key_type::BLOCK_GROUP_ITEM as BLOCK_GROUP_ITEM_KEY;
use crate::chunk::key_type::{
    FREE_SPACE_BITMAP as FREE_SPACE_BITMAP_KEY, FREE_SPACE_EXTENT as FREE_SPACE_EXTENT_KEY,
    FREE_SPACE_INFO as FREE_SPACE_INFO_KEY,
};

impl Filesystem {
    /// The free-space tree leaves that describe any of `addresses`.
    fn free_space_leaves_for(&self, addresses: &[u64]) -> Result<BTreeSet<u64>> {
        let Ok(root) = self.tree_root(objectid::FREE_SPACE_TREE) else {
            return Ok(BTreeSet::new());
        };
        let groups = self.block_groups()?;
        let spans: Vec<(u64, u64)> = groups
            .iter()
            .filter(|g| addresses.iter().any(|a| g.contains(*a)))
            .map(|g| (g.start, g.end()))
            .collect();
        if spans.is_empty() {
            return Ok(BTreeSet::new());
        }

        // Same rule as the extent tree: an insert lands in the last
        // leaf that begins at or before its key, which is not the same
        // as the leaf whose existing keys bracket it.
        let mut keys: Vec<u64> = Vec::new();
        for (start, end) in &spans {
            keys.push(*start);
            for a in addresses.iter().filter(|a| **a >= *start && **a < *end) {
                keys.push(*a);
            }
        }
        self.leaves_holding(root, &keys)
    }

    /// Refuse a plan that touches a block group whose free-space records
    /// span more than one leaf (#177).
    ///
    /// [`Filesystem::apply_free_space`] rewrites a group from the leaf
    /// holding its `FREE_SPACE_INFO`, and writes every run of the group
    /// back into that leaf. A leaf can end among a group's records, and
    /// then the next leaf keeps its own copies of the group's tail: the
    /// tree records that free space twice, and out of key order across
    /// the boundary, which `btrfs check` reports as "free space extent
    /// ... overlaps with previous". Moving records between leaves, and
    /// the keys above them, is not implemented, so the plan is refused.
    ///
    /// Measured on a kernel-made 4 KiB-node volume: with ~160 records to
    /// a leaf, a boundary lands inside some group as soon as a few groups
    /// are fragmented (`tests/free_space_straddle.rs`).
    fn refuse_free_space_straddle(&self, plan: &Plan, data: &DataWrite) -> Result<()> {
        let Ok(root) = self.tree_root(objectid::FREE_SPACE_TREE) else {
            return Ok(());
        };
        let moved: Vec<u64> = plan
            .released()
            .into_iter()
            .chain(plan.allocated())
            .chain(data.addresses())
            .collect();
        let touched: Vec<crate::block_group::BlockGroup> = self
            .block_groups()?
            .into_iter()
            .filter(|g| moved.iter().any(|a| g.contains(*a)))
            .collect();
        if touched.is_empty() {
            return Ok(());
        }

        // Which leaves hold records of each touched group, in one walk.
        let mut leaves: BTreeMap<u64, BTreeSet<u64>> = BTreeMap::new();
        self.for_each_tree_block(root, &mut |at, block, _| {
            let Some(items) = block.body.items() else {
                return;
            };
            for item in items {
                if !matches!(
                    item.key.key_type,
                    FREE_SPACE_INFO_KEY | FREE_SPACE_EXTENT_KEY | FREE_SPACE_BITMAP_KEY
                ) {
                    continue;
                }
                if let Some(group) = touched.iter().find(|g| g.contains(item.key.objectid)) {
                    leaves.entry(group.start).or_default().insert(at);
                }
            }
        })?;

        match leaves.iter().find(|(_, held)| held.len() > 1) {
            Some((start, held)) => Err(Error::UnsupportedFeature(format!(
                "the free-space records of the block group at {start} span {} leaves, and \
                 rewriting a group across a leaf boundary is not implemented",
                held.len()
            ))),
            None => Ok(()),
        }
    }

    /// Rewrite a free-space tree leaf so it describes what the plan
    /// leaves behind.
    ///
    /// The free-space tree is the complement of the extent tree, so a
    /// transaction that moves blocks changes both. A leaf left saying
    /// where things used to be is what `btrfs check` reports as "cache
    /// appears valid but isn't".
    ///
    /// # The two things a first attempt got wrong
    ///
    /// **A leaf is not per-block-group.** Measured: the whole tree can
    /// be one leaf holding a `FREE_SPACE_INFO` for each of several
    /// groups, each followed by that group's `FREE_SPACE_EXTENT`s in
    /// objectid order. So a leaf is rewritten group by group, and every
    /// group in it must come out again.
    ///
    /// **An `INFO` may name a group that no longer exists.** The tree
    /// outlives its block groups and `btrfs check` calls that correct —
    /// see `docs/cow-transaction.md`. Those entries are carried through
    /// untouched: there is no block group to derive a free set from, and
    /// dropping them would delete something the kernel put there.
    ///
    /// # Errors
    ///
    /// [`Error::UnsupportedFeature`] for a leaf holding a
    /// `FREE_SPACE_BITMAP`, which needs its bits rewritten rather than
    /// its extents and is not implemented. Refusing beats writing
    /// extents where the kernel will read bits. And for a group the plan
    /// touches that no block group item describes: its free set cannot
    /// be derived, and carrying its records leaves the plan's
    /// allocations recorded as free.
    ///
    /// A group whose records continue into the next leaf is refused
    /// before this runs, by [`Filesystem::refuse_free_space_straddle`].
    fn apply_free_space(
        &self,
        items: Vec<crate::leaf_edit::OwnedItem>,
        plan: &Plan,
        data: &DataWrite,
    ) -> Result<Vec<crate::leaf_edit::OwnedItem>> {
        use crate::block_group::FreeExtent;
        use crate::chunk::DiskKey;
        use crate::leaf_edit::OwnedItem;

        if items
            .iter()
            .any(|i| i.key.key_type == FREE_SPACE_BITMAP_KEY)
        {
            return Err(Error::UnsupportedFeature(
                "this free-space tree records a block group as a bitmap, and rewriting \
                 bits is not implemented"
                    .to_string(),
            ));
        }

        let groups = self.block_groups()?;
        // Each range the transaction frees or takes: a node for every
        // tree block, and a data extent's own length for each of those.
        let nodesize = self.sb.nodesize as u64;
        let released: Vec<(u64, u64)> = plan
            .released()
            .into_iter()
            .map(|at| (at, nodesize))
            .chain(data.moves.iter().map(|m| (m.old, m.old_len)))
            .collect();
        let allocated: Vec<(u64, u64)> = plan
            .allocated()
            .into_iter()
            .map(|at| (at, nodesize))
            .chain(data.moves.iter().map(|m| (m.new, m.len)))
            .collect();

        let mut out: Vec<OwnedItem> = Vec::with_capacity(items.len());
        let mut i = 0usize;
        while i < items.len() {
            let item = &items[i];
            if item.key.key_type != FREE_SPACE_INFO_KEY {
                // An extent item with no info before it: carry it.
                out.push(item.clone());
                i += 1;
                continue;
            }

            // This group's span, and the run of items belonging to it.
            let start = item.key.objectid;
            let end = start + item.key.offset;
            let mut j = i + 1;
            while j < items.len()
                && items[j].key.key_type != FREE_SPACE_INFO_KEY
                && items[j].key.objectid < end
            {
                j += 1;
            }

            let touched = released
                .iter()
                .chain(allocated.iter())
                .any(|(a, _)| *a >= start && *a < end);
            let group = groups.iter().find(|g| g.start == start);

            match (touched, group) {
                // Untouched: carry the whole run through exactly as it
                // was, including an INFO naming a group that no longer
                // exists.
                (false, _) => out.extend_from_slice(&items[i..j]),
                // Touched, but no block group starts here: the free-space
                // tree and the extent tree disagree about where the group
                // is. Carrying the old records would leave everything the
                // plan allocates inside it recorded as free.
                (true, None) => {
                    return Err(Error::UnsupportedFeature(format!(
                        "the free-space tree records a group at {start} that no block group \
                         item describes, and the plan allocates or releases inside it"
                    )))
                }
                (true, Some(group)) => {
                    // What is free now, plus what the plan releases,
                    // minus what it takes.
                    let mut free = self.free_extents(group)?;
                    for &(start, len) in released.iter().filter(|(a, _)| group.contains(*a)) {
                        free.push(FreeExtent { start, len });
                    }
                    free.sort();

                    // The same join the free-space reader does, and the
                    // same function: two copies of "runs that touch
                    // become one" is two places for it to stop being
                    // true.
                    let mut runs: Vec<FreeExtent> = crate::block_group::merge_adjacent(free);
                    for &(at, len) in allocated.iter().filter(|(a, _)| group.contains(*a)) {
                        runs = runs.into_iter().flat_map(|r| carve(r, at, len)).collect();
                    }
                    runs.retain(|r| r.len > 0);

                    // A FREE_SPACE_INFO item that cannot hold its own
                    // extent count is refused rather than passed through
                    // with the old one. The `if` that used to guard this
                    // wrote the count when the item was long enough and
                    // said nothing when it was not — so a short item kept
                    // a STALE count while its runs were rewritten
                    // underneath it, and the free-space tree then
                    // disagreed with itself with no signal that anything
                    // had gone wrong.
                    //
                    // The offsets come from block_group::free_space_info,
                    // which is where the reader already gets them; this
                    // side was spelling them out by hand.
                    let mut info = item.clone();
                    if info.data.len() < crate::block_group::free_space_info::SIZE {
                        return Err(Error::UnsupportedFeature(
                            "FREE_SPACE_INFO item is shorter than the structure it declares"
                                .to_string(),
                        ));
                    }
                    let count_at = crate::block_group::free_space_info::EXTENT_COUNT;
                    info.data[count_at..count_at + 4]
                        .copy_from_slice(&(runs.len() as u32).to_le_bytes());
                    out.push(info);
                    for run in runs {
                        out.push(OwnedItem {
                            key: DiskKey {
                                objectid: run.start,
                                key_type: FREE_SPACE_EXTENT_KEY,
                                offset: run.len,
                            },
                            data: Vec::new(),
                        });
                    }
                }
            }
            i = j;
        }
        Ok(out)
    }
}

/// `run` with `[at, at + len)` taken out of it.
fn carve(
    run: crate::block_group::FreeExtent,
    at: u64,
    len: u64,
) -> Vec<crate::block_group::FreeExtent> {
    use crate::block_group::FreeExtent;
    let end = at + len;
    if end <= run.start || at >= run.end() {
        return vec![run];
    }
    let mut out = Vec::new();
    if at > run.start {
        out.push(FreeExtent {
            start: run.start,
            len: at - run.start,
        });
    }
    if end < run.end() {
        out.push(FreeExtent {
            start: end,
            len: run.end() - end,
        });
    }
    out
}

/// Library tests that need the fs-linux-test-harness VM.
///
/// THE NAME IS LOAD-BEARING. `chore test:unit` runs the library's own
/// tests with `--skip needs_host::`, because that tier must pass on a
/// runner with no VM and no fixtures — which is what proves the split.
/// These run in the whole-release suite instead, where the VM is up.
/// A module named anything else would be run there, fail to reach a
/// guest, and take the unit tier down with it.
#[cfg(test)]
mod needs_host {
    use crate::fs::Filesystem;
    use crate::superblock::SUPER_OFFSETS;
    use fs_btrfs_test_support::{oracle, temp_path};
    use fs_core::FileDevice;
    use std::collections::BTreeSet;
    use std::sync::Arc;

    /// A new tree block never lands on a superblock copy.
    ///
    /// The extent tree doesn't record the superblock copies, so the free
    /// runs it implies include them. The kernel and btrfs-progs exclude
    /// the `stripe_len` window holding each copy from every block group
    /// (`exclude_super_stripes`, #175). A 256 MiB `mkfs.btrfs` image puts the
    /// first copy of its DUP metadata chunk across 64 MiB, where
    /// `Filesystem::commit` writes the second superblock after the tree
    /// blocks. This allocates every block the group has and checks each
    /// one's copies against that window.
    ///
    /// `mkfs.btrfs` runs in the harness VM, like every oracle tool, so
    /// the image is made by one version of btrfs-progs on every machine
    /// — and the scratch directory is inside this repository, which is
    /// the only part of the host the guest can see.
    #[test]
    fn no_tree_block_is_allocated_over_a_superblock_copy() {
        let dir = std::path::PathBuf::from(temp_path!("alloc-over-super"));
        std::fs::create_dir_all(&dir).unwrap();
        let img = dir.join("fs.img");
        std::fs::File::create(&img)
            .unwrap()
            .set_len(256 * 1024 * 1024)
            .unwrap();
        let made = oracle("mkfs.btrfs")
            .args(["-q", "-f", "-s", "4096", "-n", "16384"])
            .arg(&img)
            .output();
        assert!(
            made.status.success(),
            "{}",
            String::from_utf8_lossy(&made.stderr)
        );

        let fs = Filesystem::mount(Arc::new(FileDevice::open(&img).unwrap())).unwrap();
        let nodesize = fs.sb.nodesize as u64;

        // The windows, from the chunk items: for each stripe on a device
        // that holds a copy, the stripe_len row the copy falls in.
        let mut windows = Vec::new();
        for chunk in fs.map.chunks().iter().filter(|c| c.is_metadata()) {
            for stripe in &chunk.stripes {
                let per_stripe = chunk.length; // DUP and single: one stripe is the chunk
                for &off in &SUPER_OFFSETS[1..] {
                    if off >= stripe.offset && off < stripe.offset + per_stripe {
                        let row = (off - stripe.offset) / chunk.stripe_len * chunk.stripe_len;
                        windows.push((stripe.offset + row, chunk.stripe_len));
                    }
                }
            }
        }
        assert!(
            !windows.is_empty(),
            "the fixture no longer puts a metadata stripe across a superblock copy"
        );

        let mut taken = BTreeSet::new();
        while let Ok(at) = fs.next_free_block(&taken) {
            taken.insert(at);
            for mirror in 0..fs.map.mirrors_at(at).unwrap() {
                let m = fs.map.map_mirror(at, mirror).unwrap();
                for &(start, len) in &windows {
                    assert!(
                        m.physical + nodesize <= start || m.physical >= start + len,
                        "the block at {at} has a copy at {}, inside the superblock \
                         window [{start}, +{len})",
                        m.physical
                    );
                }
            }
        }
        assert!(taken.len() > 1000, "allocated only {} blocks", taken.len());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A touched group that no block group item describes is refused, not
    /// carried through with its old records.
    ///
    /// SELF-CONSISTENCY ONLY, and it says so: no oracle can make this
    /// input. The kernel removes a group's `FREE_SPACE_INFO` with the
    /// group, and a stale one left by mkfs covers a range no chunk maps,
    /// so no plan ever allocates or releases inside it. The refusal is
    /// there because carrying the records would leave every block the
    /// plan allocates in that range recorded as free, and this is the
    /// only way to reach it: an INFO item one block past a real group's
    /// start, so no group starts where it says, over addresses the plan
    /// moves.
    #[test]
    fn a_touched_group_no_block_group_item_describes_is_refused() {
        use crate::chunk::DiskKey;
        use crate::leaf_edit::OwnedItem;
        use crate::transaction::{Plan, Rewrite};

        let dir = std::path::PathBuf::from(temp_path!("fst-no-group"));
        std::fs::create_dir_all(&dir).unwrap();
        let img = dir.join("fs.img");
        std::fs::File::create(&img)
            .unwrap()
            .set_len(256 * 1024 * 1024)
            .unwrap();
        let made = oracle("mkfs.btrfs")
            .args(["-q", "-f", "-s", "4096", "-n", "16384"])
            .arg(&img)
            .output();
        assert!(
            made.status.success(),
            "{}",
            String::from_utf8_lossy(&made.stderr)
        );

        let fs = Filesystem::mount(Arc::new(FileDevice::open(&img).unwrap())).unwrap();
        let nodesize = fs.sb.nodesize as u64;
        let group = fs
            .block_groups()
            .unwrap()
            .into_iter()
            .find(|g| g.holds_metadata())
            .expect("a metadata block group");
        let start = group.start + nodesize;
        assert!(
            fs.block_groups().unwrap().iter().all(|g| g.start != start),
            "a block group starts at {start}, so the INFO item below would describe it"
        );

        let items = vec![OwnedItem {
            key: DiskKey {
                objectid: start,
                key_type: super::FREE_SPACE_INFO_KEY,
                offset: group.length - nodesize,
            },
            data: vec![0; crate::block_group::free_space_info::SIZE],
        }];
        let plan = Plan {
            rewrites: vec![Rewrite {
                old: start + nodesize,
                new: start + 2 * nodesize,
                owner: crate::chunk::objectid::FS_TREE,
                level: 0,
            }],
        };
        let outcome = fs.apply_free_space(items, &plan, &super::DataWrite::default());
        let _ = std::fs::remove_dir_all(&dir);
        let error = outcome.expect_err(
            "the plan moves blocks inside a group no block group item describes, and its \
             records were carried through as if nothing inside it had changed",
        );
        assert!(
            error.to_string().contains("no block group item describes"),
            "refused, but not for the missing group: {error}"
        );
    }
}
