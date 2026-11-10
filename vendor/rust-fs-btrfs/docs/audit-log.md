# Write-path audit log

Every pass over this driver's write paths asks one question:

> Is there an edit that can be applied to a filesystem the driver has **misread**?

The question is not about a missing bounds check. It's about a value read one way and then used another, on an ordinary filesystem. That's the shape of every corruption defect found in this family of drivers so far. The same log, in the same format, is kept in `rust-fs-ext4`, `rust-fs-xfs`, `rust-fs-btrfs` and `rust-fs-ntfs`.

Each pass is one entry, newest first. An entry names:
- the commit it read;
- every module it looked at, with the result, "none found" included;
- each finding with its issue;
- what it didn't reach, so the next pass knows where to start.

## 2026-09-18: many writes over many commits

Read at `c083350` (#63), by running rather than by reading, which is the second
pass over the same question. A probe drove the one public mutation this
driver has — an in-place `nodatacow` write — across many files and many
commits, and had the kernel and `btrfs check` judge the result.

| What the probe drove | Result |
|---|---|
| 2,500 writes per volume, five per mount, so 500 commits | none found |
| offsets and lengths varied per step, over 40 files of 256 KiB | none found |
| a kernel mount, digest and `btrfs check` every 25 commits | accepted every time |

Three geometries: the default, `-n 4096 -m dup -d single`, and `-M` (mixed
block groups) on a half-size volume, so metadata allocation is tighter. 7,500
writes in all, none refused, and every check clean.

**What that covers.** Each mount is a commit, so this is mostly the commit
path: `transaction.rs`'s render and apply, the metadata allocator in
`block_group.rs`, `leaf_edit.rs`, the superblock image and its backup ring in
`super_write.rs` and `commit.rs`. The first pass found #175 in exactly that
area, an allocator taking extent-tree gaps as free.

**The oracle can fail, which is the part worth stating.** The kernel's digest
of the files is compared between rounds, and a round whose digest matches the
last one fails the probe: a clean `btrfs check` over a volume nothing was
written to would otherwise read as a pass.

**Findings:** none.

**Not reached, for the next pass:**
- everything a write cannot reach from this API: creates, unlinks, renames,
  directory edits and truncation are not implemented, so the paths that would
  change a tree's shape were not exercised;
- the free-space tree straddle in #177, which is parked with its findings on
  the issue — the probe's writes do not change free space, so it cannot see
  it;
- snapshots and reflinked extents beyond what #173 and #174 covered.

## 2026-09-17: write, allocation, transaction and commit paths

Read at `ece3fa9` (#63).

| Module | Result |
|---|---|
| `src/write.rs` (in-place nodatacow write) | #173 |
| `src/transaction.rs`: `next_free_block` | #175 |
| `src/block_group.rs`: free extents, `find_metadata_block` | #175 |
| `src/transaction.rs`: `apply_free_space` | #177 |
| `src/transaction.rs`: `plan_transaction`, `apply_records` | #178 |
| `src/transaction.rs`: `render_plan` | none found |
| `src/leaf_edit.rs` | none found |
| `src/extent_write.rs` | none found |
| `src/commit.rs`, `src/super_write.rs` | none found |

**Findings:**
- **#173, fixed in #174.** `write_at` took an extent item's `refs == 1` to mean one reader. A snapshot of a node-rooted tree leaves data extents at 1, so the write changed the snapshot's copy. Reproduced on a kernel-made snapshot.
- **#175, fixed in #176.** The allocator took the extent tree's gaps as free, including the `stripe_len` rows holding superblock copies. On a 256 MiB volume it handed out the block the commit's second superblock is written over. Reproduced locally.
- **#177, from reading the code.** The free-space-tree rewrite assumes a block group's records sit in one leaf.
- **#178, from reading the code.** A transaction releases a tree block by deleting its extent record, whatever its reference count. That's reachable through the public planner on a snapshotted tree.

**Looked at and not a finding:**
- The commit builds its superblock from the raw primary, but a writable mount is refused unless copy 0 is the one chosen (#146).
- `render_plan` drops leaf items whose data can't be located, but leaves are validated when read.
- `leaf_edit::insert_or_split` splits by count, not bytes. It has no production caller, and `build_leaf` refuses an oversized leaf.
- `window_inside_extent` bounds a write by the extent tree's own length (#89).

**Not reached:** `src/capi.rs`'s write entry points beyond what they call, `src/xattr.rs` and `src/tree_write.rs`'s node builder.
