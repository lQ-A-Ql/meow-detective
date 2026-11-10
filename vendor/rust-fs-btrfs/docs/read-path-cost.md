# What a read costs

Measured by `tests/read_path_cost.rs`, which counts **calls to the
device** rather than wall time. Wall time on a laptop with a warm page
cache says more about the laptop than the driver; call counts are
deterministic — the same image walked the same way makes the same calls
every time — so they can be compared across months and asserted on.

Wall time is printed beside them because it is what a user feels. It is
not what anything is judged by, except where it is the only thing left
to judge by — which, as it turns out, is this driver's situation.

## 2026-09-06 — the first measurement

Two fixtures, because they say different things.

### `btrfs-rich.img` — a varied tree, 8 entries, 6 files

| shape | reads | bytes | wall |
|---|---:|---:|---:|
| mount | 4 | 53 KB | 3523 µs |
| walk | 0 | 0 | 54 µs |
| stat | 0 | 0 | 21 µs |
| read | 8 | 2.13 MB | 27605 µs |

### `btrfs-deep4k.img` — 20,000 files in `/many/`, walk bounded at 400

| shape | reads | bytes | wall |
|---|---:|---:|---:|
| mount | 2573 | 10.5 MB | 416918 µs |
| walk | 0 | 0 | 375431 µs |
| stat | 0 | 0 | 373342 µs |
| read | 0 | 0 | 371542 µs |

## What the numbers say

**Every metadata read happens at mount.** `Filesystem` loads every item
of the fs tree into a `BTreeMap` when it opens the volume, and answers
from memory afterwards. So a walk, a stat and a read of already-loaded
items make **zero** calls to the device. That is not a cache warming
up; there is nothing left to fetch.

**The mount cost scales with the volume, not with the work.** Four
reads and 53 KB for the small fixture; 2573 reads, 10.5 MB and **417
milliseconds** for one with 20,000 files, before a single question has
been asked. A volume with a million files is the same arithmetic with
two more zeros, and the memory it holds grows the same way. Opening a
filesystem in Finder to look at one directory pays for all of it.

**The remaining time is not I/O at all.** The walk of 401 items takes
375 ms and touches the device zero times. So does the stat of 400
paths, and so does the read. Three different shapes, each about 373 ms,
none of them waiting on a disk: the time is spent in the driver,
scanning the item map. That is `#64` — a lookup that scans rather than
descends — and it is now the dominant cost of every operation.

## Why there is no block cache

`rust-fs-core`'s `CachingDevice` was wired in and then deliberately turned
off; `DEFAULT_CACHE_BLOCKS` is `0`. Two measured reasons:

1. **There is nothing for it to serve.** The repeat metadata reads a
   cache exists to absorb do not happen — the eager load already
   removed them.
2. **It makes the mount worse.** Metadata is read a node at a time, and
   a node is `nodesize`, typically four sectors. A cache sized in
   sectors splits one call into four: the `rich` fixture's mount asked
   the device for 4 reads uncached and **14** cached.

`Filesystem::mount_with_cache` stays, so the measurement can take both
passes and so the decision can be re-taken against a number rather than
an opinion. If the eager load is ever replaced by lazy descent, a cache
becomes worth having and that constant should change with it.

## What to fix, in order

1. **The lookup scan (#64).** Every shape now costs the same ~370 ms
   with no I/O, which means the item map is being scanned rather than
   searched. Biggest gain available and it touches no on-disk format.
2. **The eager mount.** 417 ms and 10.5 MB for 20,000 files is a
   figure that only goes one way as volumes get real. Making the tree
   lazy is a larger change and it is the one that would make a block
   cache worth having.

## 2026-09-17 — lazy descent (#67), and the lookup by key (#64)

Both fixes the list above asked for are in. `Filesystem::lookup` fetches
a name's `DIR_ITEM` by key (#64), and the mount no longer loads the fs
tree: items are found by descending the tree when asked, and the blocks
a descent reads are kept, already verified, in a node cache of
`NODE_CACHE_BLOCKS` (1024) by logical address, cleared by a commit. A
cache hit skips the read and the checksum; a parent's generation is still
compared. 20,000 lookups in one directory take 260 ms in release, where
re-verifying each cached node took 1.1 s and the in-memory map 20 ms.

Measured on an image of `btrfs-deep4k`'s shape built without a VM
(`mkfs.btrfs -s 4096 -n 4096 --rootdir` over 20,000 one-line files in
`/many/`), because the VM fixture cannot be built on this host. The
figures are therefore not the table above's image; the before and after
are the same one.

| shape | before: eager mount | after: lazy descent |
|---|---:|---:|
| mount | 2937 reads, 12.0 MB, 118 ms | **5 reads, 20 KB, 0.5 ms** |
| walk (401 items) | 0 reads | 684 reads, 2.8 MB |
| stat (400 paths) | 0 reads | 0 reads |
| read (400 files) | 0 reads | 0 reads |

The mount now costs the bootstrap and nothing that scales with the
volume. The walk pays for the leaves it lists, once; the stat that
follows re-resolves every path through interior nodes the walk already
cached and reaches the device zero times, which is the property the
eager load was written to protect. These files are inline in their
leaves, so reading them is free once the walk has been through.

`DEFAULT_CACHE_BLOCKS` stays `0`: the node cache sits above the device
and holds whole nodes, which is the sizing the sector cache got wrong.

## How to take the measurement again

```sh
cargo test --release --test read_path_cost -- --nocapture
```

It measures every one of `btrfs-rich.img`, `btrfs-deep4k.img` and
`btrfs-commit.img` from `test-disks/`, one block of output each, so both
tables above come from the one command. It does not skip: a missing
fixture fails it, naming `chore fixtures`, which builds them with the
real kernel inside the fs-linux-test-harness VM.
