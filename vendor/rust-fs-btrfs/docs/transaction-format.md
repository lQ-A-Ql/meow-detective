# What a Btrfs transaction writes

Established by differential analysis against filesystems the kernel wrote — never
from implementation source. Every layout below was read off disk and corroborated
against the reference tooling's own dump of the same structure.

This is what a driver must produce to commit a write. It is written down because the
alternative is rediscovering it, and because several parts are not what a reasonable
person would guess.

## The control that makes the rest trustworthy

**A no-op transaction writes zero bytes.** Mount, sync, snapshot the image, sync again,
snapshot again: `cmp` over the whole 512 MiB reports no difference.

That matters more than any single finding. It means every byte in a subsequent diff is
caused by the operation under test, with no background noise to explain away.

## The superblock is the commit point

Not inferred — demonstrated. Take an image with a committed transaction, write only the
*old* 4 KiB superblocks back over both copies, fix their checksums, and the transaction
is gone: the checker reports `no error found`, and the file's `EXTENT_DATA` item is no
longer there.

Everything else on disk is still present. It is simply unreferenced.

## Copy-on-write is literal

A `chmod` on a filesystem whose fs tree is at level 2 rewrote **exactly the root-to-leaf
path** and nothing else:

```
FS_TREE      level 2 root -> level 1 -> leaf     3 blocks
EXTENT_TREE  level 1 root + 5 leaves             6 blocks
FREE_SPACE_TREE leaf                             1 block
ROOT_TREE    leaf                                1 block
CSUM / DEV / UUID / DATA_RELOC / CHUNK           untouched
```

The extent tree is the amplifier: eleven frees and eleven allocations scatter across it.
It is also **self-referential** — the extent-tree leaf a commit writes contains the
`METADATA_ITEM` describing itself.

Nothing is overwritten in place. The old blocks remain byte-identical on disk.

One detail that misleads a coarse diff: within a 16 KiB node only the first and last
4 KiB change, because items grow from the front and item data from the back. The middle
keeps stale bytes, so diffing at 4 KiB granularity undercounts.

## Superblock fields that move

| offset | field | when |
|---|---|---|
| 0x000 | `csum` | every commit |
| 0x048 | `generation` | every commit, +1 |
| 0x050 | `root` | every commit |
| 0x233 | `uuid_tree_generation` | every commit, tracks `generation` even when the UUID tree is untouched |
| 0xB2B + 168·((gen−1) mod 4) | one `btrfs_root_backup` slot | every commit |
| 0x078 | `bytes_used` | when usage changes |
| 0x0C6 | `root_level` | when the root tree changes depth |
| 0x058 / 0x0A4 / 0x0D9 | `chunk_root`, its generation, `dev_item.bytes_used` | when a chunk is allocated |
| 0x22B | `cache_generation` | zero with the free-space tree; tracks `generation` under `space_cache=v1` |

`sys_chunk_array` carries only SYSTEM chunks and does not move when a data chunk is
allocated.

**Backup slot index is `(generation − 1) mod 4`**, confirmed across six independent
generations.

**Invariant worth asserting in a writer:** `bytes_used` equals the sum of every
`BLOCK_GROUP_ITEM.used`.

## Checksums

All three computed independently and matched exactly:

```
tree block    crc32c(block[32..nodesize])   LE in bytes 0..4, rest of the 32-byte field zero
superblock    crc32c(sb[32..4096])
data sector   crc32c(sector[0..sectorsize])
```

Tree-block `flags` is `0x0100000000000001` — bit 0 is WRITTEN, the top byte is backref
revision 1. A writer must set both.

## Extent items

**Tree block, with `SKINNY_METADATA`.** Key `(bytenr, METADATA_ITEM=169, level)`,
itemsize **33**:

```
 0  u64 refs
 8  u64 generation
16  u64 flags            2 = TREE_BLOCK
24  u8  inline ref type  176 = TREE_BLOCK_REF
25  u64 offset           owner root objectid
```

So `btrfs_extent_item` is 24 bytes followed by a packed `{u8, u64}` of 9. There is no
`btrfs_tree_block_info`: skinny metadata puts the level in the key's offset instead.

**Data extent.** Key `(bytenr, EXTENT_ITEM=168, length)`, itemsize **53**:

```
 0..24  extent item, flags = 1 (DATA)
   24   u8 type = 178 EXTENT_DATA_REF
25..53  btrfs_extent_data_ref: root, objectid, offset, count
```

Note the asymmetry — the 28-byte data ref follows the type byte directly, so the 9-byte
`{type, offset}` shape does **not** apply here.

**Sharing.** A snapshot grows the data extent's item to 82 bytes with `refs 2` and two
inline `EXTENT_DATA_REF`s. Hardlinks do not multiply refs; only distinct inodes do.

**Inline refs spill.** Past a cap the refs become standalone items, key
`(bytenr, EXTENT_DATA_REF=178, hash)`, itemsize 28. Observed caps: 981 bytes at
nodesize 16384, 198 at 4096. Both fit `((nodesize − 101) >> 4) − 25`, which is a
two-point fit and should be treated as provisional.

## Checksum tree

```
key   (objectid = -10 EXTENT_CSUM_OBJECTID, type = 128 EXTENT_CSUM_KEY,
       offset  = logical byte address of the first sector)
data  csum_size bytes per sector, packed contiguously, no header
```

A 64 KiB extent produced one item of itemsize 64 covering sixteen sectors. Items split
and merge by logical range, not by file. The tree is touched only when data is written.

## The smallest correct transaction

For a level-0 filesystem, one metadata field changed, DUP metadata:

1. Allocate three tree blocks from the metadata block group.
2. Write the **fs-tree leaf**, the **extent-tree leaf** (drop three old `METADATA_ITEM`s,
   add three new, adjust `BLOCK_GROUP_ITEM.used`) and the **root-tree leaf** (new fs-root
   bytenr, generation, `ctransid`). Each with the new generation, `flags` as above, and
   its checksum.
3. Write each to **both DUP stripes**.
4. Barrier.
5. Write every in-range superblock copy: `generation + 1`, new `root`, `root_level`,
   `bytes_used`, `uuid_tree_generation`, the backup slot, the copy's own `bytenr`, fresh
   checksum.

Three metadata blocks, two mirrors, two or three superblocks. Confirmed by mounting
`space_cache=v1` and running one `touch`.

Add to that: a free-space-tree leaf, a checksum-tree leaf and data blocks for data
writes, and the whole root-to-leaf path on deeper trees.

## Two things a writer can exploit

**The free-space tree can be declined.** Clearing `FREE_SPACE_TREE_VALID` (compat_ro
bit 1, `0x3` → `0x1`) makes the kernel log *"free space tree is invalid / rebuilding
free space tree"* and repair it on the next read-write mount. A first-generation writer
can skip maintaining it rather than getting it wrong.

**`log_root` must be zero.** `fsync` writes a log tree and sets `log_root` *without*
bumping `generation`; the checker ignores the log and reports the committed state. A
driver must refuse to write when `log_root != 0` — which this one already does at mount.

## Superblock copies

Offsets 65536, 67108864 and 274877906944. All in-range copies are updated in the same
commit and are byte-identical except `bytenr` at 0x30 and the resulting checksum. A copy
is written only if it fits on the device.

A fourth copy at 1 PiB is commonly cited and was **not** verified here — the test
environment caps files below that size.

## Left open

| question | what would settle it |
|---|---|
| the 1 PiB superblock copy | a sparse 1.1 PiB device via device-mapper |
| `SHARED_BLOCK_REF` (type 182) layout | snapshot a subvolume whose tree is level ≥ 1 |
| `FREE_SPACE_BITMAP` (type 200) layout | fragment a block group past the extent-vs-bitmap threshold |
| the inline-ref cap formula | a third nodesize |
| ~~write ordering and barrier placement~~ | **settled** — see "The order, observed" below |

Ordering used to be the one thing above that was reasoned rather than observed. It is
observed now — see below — and the reasoning was right about the shape and incomplete
about the end.

## The order, observed

`scripts/trace-commit.sh` records one live commit with `blktrace`, watching the loop
device the filesystem is mounted on. Only `D` events are kept: the moment each request
was *dispatched* to the device, which is the order the device sees.

One `touch` and one `sync`, on a filesystem `mkfs.btrfs` made with today's defaults:

```text
D WSM  76160  32     ┐
D WSM 141696  32     │ four tree blocks, as two DUP pairs
D WSM 141728 128     │   16 KiB mirrored at 76160 / 141696
D WSM  76192 128     ┘   64 KiB mirrored at 141728 / 76192
D FN                 <- flush
D WSM    128   8     superblock copy at 64 KiB
D WSM 131072   8     superblock copy at 64 MiB
D FN                 <- flush
```

Identical across three runs.

**What this confirms.** Tree blocks first, then a barrier, then the superblocks. Both
mirrors of a DUP block are written before the barrier, so a torn write to one leaves the
other and the barrier still orders both against the superblock.

**What it adds, and what nobody had reasoned.** There is a *second* flush, after the
superblocks. And the superblocks are written with **no FUA** — the flags are `WSM`,
write/sync/metadata, not the `A` that force-unit-access would show. So durability of the
commit point comes from the trailing flush rather than from FUA on the write itself. A
writer that set FUA and omitted the second flush would be doing something the kernel does
not do; one that omitted both would have no commit point at all.

**What it does not show.** This filesystem was small enough for two superblock copies,
and the trace is of a metadata-only commit — no data extents and so no checksum-tree
leaf. A data write would add both, and the document's reasoning puts them before the
tree blocks. That part is still reasoned.

The mirrors come out interleaved — `A1 B1 B2 A2` rather than `A1 A2 B1 B2` — which is the
I/O scheduler and not the filesystem. Nothing should depend on it.

## An incidental find

The first read-write mount of a filesystem made by a newer `mkfs` can set the
`BIG_METADATA` incompat bit and initialise `uuid_tree_generation`. A reader should
tolerate both states.
