# What a transaction actually does

The write path had five pieces — leaves, nodes, allocation, the record of an
allocation, and the superblock — and a sequencer that puts them on the device in the
order the kernel does. What it did not have was the part that decides *which* blocks a
change produces, because that part is recursive: recording an allocation modifies the
extent tree, and the extent tree lives in blocks that must themselves be allocated.

Reasoning about how the kernel breaks that cycle is how a writer ends up implementing
something plausible and wrong. So it was measured.

The `cow` recipe in `test-disks/guest-build-images.sh` (`chore fixtures cow`)
captures three whole images of one filesystem:

| image | what happened |
|---|---|
| `btrfs-cow-before.img` | `mkfs`, then one mount/unmount so the first-mount feature write is already done |
| `btrfs-cow-control.img` | mounted and unmounted again, **changing nothing** |
| `btrfs-cow-after.img` | one `touch` and one `sync` |

**A mount cycle that changes nothing does not always commit.** It did in the Debian
oracle VM, where the numbers below were measured; the same script on the CI runner left
the generation unmoved. Which is fair — there was nothing to write. So the control pair
is a measurement when it commits and an empty pair when it does not, and
`tests/transaction_oracle.rs` treats a pair with no transaction as nothing to check
rather than as a failure, while requiring that *something* committed.

The control is what makes the rest mean anything. Mounting read-write commits by
itself, so a before/after pair around a `touch` contains the touch *and* whatever a bare
mount cycle does. Without a pair that did nothing, all of it would be attributed to
creating the file.

`cargo run --example cow_diff -- <before> <after>` produces what follows.

## An empty commit is not empty

Changing nothing still rewrites four blocks:

```text
generation 8 -> 9
bytes_used 147456 -> 147456

tree blocks written
  dev              1
  extent           1
  free-space-tree  1
  root             1

extent tree items ADDED    4
extent tree items REMOVED  4
```

That is the fixed point, visible. Nothing the user did caused it:

- the **root tree** must be rewritten, because it names the other trees' roots and
  those addresses changed;
- the **extent tree** must be rewritten, to record the new blocks and release the old;
- the **free-space tree** must be rewritten, because what is free changed;
- the **dev tree** must be rewritten, because device usage changed.

And each of those four rewrites is itself an allocation, recorded in the same
transaction — which is what makes the count come out equal rather than growing without
bound. **Four in, four out.** The recursion terminates because a copy-on-write rewrite
is an allocation *and* a free, and the extent tree ends up recording its own new blocks.

## One `touch` adds two blocks

The same measurement around a real change, looking at the blocks written by the
transaction that created the file:

```text
tree blocks written
  dev              1
  extent           1
  free-space-tree  1
  root             1
  fs               2      <- the file
```

Six blocks: the four an empty commit costs, plus two in the fs tree. So the cost of a
change is the fixed floor plus what the change itself touches.

## The invariant that generalises

`bytes_used` did not move in either case, because four blocks were allocated and four
released. Stated so it holds when a tree does grow:

```text
bytes_used(after) - bytes_used(before)
    == (metadata items added - metadata items removed) * nodesize
```

Together with two others this is enough to check a transaction without knowing what it
was for:

1. every block **reachable** from the superblock — through the root tree, through every
   `ROOT_ITEM` it holds, down to the leaves — has a `METADATA_ITEM` naming it;
2. every recorded block is physically on the disk and checksums;
3. `bytes_used` equals the sum of every block group's `used`, and moves by the delta
   above.

Reachability rather than generation, and that distinction cost two CI runs. "Every block
stamped with the current generation must be recorded" is the obvious cheap test and it is
wrong: a block written by one transaction in a sequence is routinely superseded by the
next, and is then still on the disk, still checksumming, and correctly unrecorded.
Generation says when a block was written, not whether anything still points at it.

`tests/transaction_oracle.rs` asserts all three against the kernel's own transactions.
It is written now, before the transaction planner exists, precisely so the planner has
something to be measured against that was not written to fit it.

## What is still not measured

A commit that makes a tree grow taller, and a data write — which adds a checksum-tree
leaf and data extents, neither of which appears above. The document's earlier reasoning
puts those before the tree blocks in write order; that part is still reasoned.

## The split boundary, and why it is not copied

An earlier version of this section read *"the split policy is not half"*, inferred from
the fill distribution of a populated filesystem — the median leaf is 91-98% full, which
looks like evidence against halving. **That was wrong.** Mostly-full leaves are what
halving *produces*, once each resulting half is filled up again by later inserts. Reading
a rule off a steady state was the mistake.

So the event was captured. The `split` recipe in `test-disks/guest-build-images.sh`
adds files one at a time, committing each, and watches the fs tree's leaf count in
btrfs-progs' own dump; when it goes up, the previous image is the before and the current
one the after. At the smallest nodesize the first split arrives at file 8. The recipe
builds a second pair (`btrfs-split-vary-*`) whose items run from 12 to 232 bytes, because
with items of equal size *half the count* and *half the bytes* give the same boundary and
a measurement says nothing.

Three real splits, read from the **live** tree either side:

```text
  42 items -> 22 | 20      half + 1
  32 items -> 17 | 15      half + 1
  52 items -> 26 | 26      half
```

They do not follow one rule. `__btrfs_split_leaf` picks its boundary partly from the slot
the new item is going into, and can push items to a sibling instead of splitting at all;
reproducing it needs the insertion slot and the sibling's state, which these fixtures
underdetermine.

**And it does not need reproducing.** The split point is not part of the on-disk format.
A checksum over the wrong span is a filesystem the kernel rejects; an item offset measured
from the wrong place is a leaf it misreads; a leaf divided in a different place is simply
a different, equally valid tree. So `leaf_edit::split` halves the item count and says so,
and `tests/split_oracle.rs` checks what actually has to hold — both halves non-empty, in
key order across the boundary as well as within each, together exactly the input, and each
fitting in a block — using the items of leaves the kernel really did split. The kernel's
own boundary is recorded alongside rather than asserted.

This is the one place in the write path where byte-identity with the kernel is the wrong
bar, and noticing that took three measurements and a CI failure: the rule fitted two
splits, and the third, on a different machine, broke it.

One methodological note. The first version of the oracle found the split leaves by
scanning for the newest generation, which turns up leaves nothing points at any more; it
paired two of those and compared a 43-item "split" that never happened. Walking the live
tree from its root fixed it. Scanning is right for asking what is *on* a disk and wrong
for asking what the filesystem *is*.

## What the kernel's checker still says, and why

The write path runs end to end — plan, close, render, record, commit — and
`examples/write_transaction.rs` performs one transaction on a copy of a filesystem.
`btrfs check` on the result:

```text
[1/7] checking root items          ok
[2/7] checking extents             ok
[3/7] checking free space tree     ok
[4/7] checking fs roots            ok
[5/7] checking only csums items    ok
[6/7] checking root refs           ok

found 147456 bytes used, no error found
```

And the kernel then mounts it read-write, writes a file, unmounts cleanly, and `btrfs
check` passes again afterwards. That is the gate that cannot be satisfied by agreeing
with ourselves: every other check here compares against bytes the kernel already wrote,
and this one asks it to accept bytes we wrote.

The superblock's `FREE_SPACE_TREE_VALID` bit is cleared when a commit says it did not
maintain the cache — the state the format defines for exactly this, and it makes the
kernel rebuild the cache on the next read-write mount. `btrfs check` verifies the cache's
*contents* regardless of the bit, so it still reports the discrepancy.

### An attempt that did not work, and what it cost

The obvious fix is to recompute each affected block group's free set — derive it from the
extent tree, add what the plan releases, remove what it takes — and rewrite the
`FREE_SPACE_EXTENT` items from that. It is wrong, and the checker said so precisely:

```text
before the attempt:  wanted bytes    81920, found 49152
after the attempt:   wanted bytes 28164096, found 49152
```

A 28 MB free run where 48 KB was expected means the rewrite replaced a leaf's items with
a free set covering ground that leaf is not responsible for. The assumption underneath it
— that a `FREE_SPACE_INFO` item and the `FREE_SPACE_EXTENT` items for its block group live
together in one leaf, so a leaf can be rewritten from the group alone — is not something
that was measured. It was assumed, and the layout does not oblige.

So the attempt was reverted rather than shipped. What settles it is the same method that
settled everything else here: measure how the kernel distributes `FREE_SPACE_INFO` and
`FREE_SPACE_EXTENT` items across leaves, on a filesystem large enough for the tree to have
more than one, and rewrite from that rather than from a guess. The bitmap form
(`FREE_SPACE_BITMAP`) has not been measured at all.

### The free-space tree outlives its block groups

Settled before attempting the rewrite again, because the failed attempt tripped over it.

The free-space tree on the fixture names **five** block groups. The extent tree has
**three**:

```text
extent tree      13631488  22020096  30408704
free-space tree   1048576   5242880  13631488  22020096  30408704
                  ^^^^^^^   ^^^^^^^  no BLOCK_GROUP_ITEM for either
```

This is not a reader bug, and it was checked against the reference tools rather than
argued about: `btrfs inspect-internal dump-tree -t 2` counts three
`BLOCK_GROUP_ITEM`s and `-t 10` lists five `FREE_SPACE_INFO`s, exactly as this driver
reports. `btrfs check` on the same image says **"no error found"**.

So a free-space tree may hold entries for block groups that have been removed, and the
filesystem is correct. Anything rewriting that tree has to tolerate it. The reverted
attempt did not: it looked each `FREE_SPACE_INFO` up by objectid, found no group for two
of the five, and bailed out — which is part of how it came to write a free set covering
ground it had no business describing.

Worth stating as a rule, because it generalises past this tree: **an item naming
something is not a guarantee that the something exists.** The checks in
`tests/transaction_oracle.rs` are deliberately one-directional for the same reason —
every reachable block must be recorded, and every recorded block must be present, but
neither says the two sets are equal.

### The free-space tree, second attempt

The first attempt (above) was reverted. What it was missing, both measured rather than
reasoned:

**A leaf is not per-block-group.** The whole tree can be one leaf holding a
`FREE_SPACE_INFO` for each of several groups, each followed by that group's
`FREE_SPACE_EXTENT`s in objectid order:

```text
INFO    1048576   len 4194304      <- no block group
  EXTENT 1048576  len 4194304
INFO    5242880   len 8388608      <- no block group
  EXTENT 5242880  len 8388608
INFO   13631488   len 8388608
  EXTENT 13631488 len 8388608
INFO   22020096   len 8388608
  EXTENT 22020096 len 16384
  EXTENT 22052864 len 8355840
INFO   30408704   len 33554432
  EXTENT 30408704 len 16384
  ... six runs
```

So a leaf is rewritten group by group, and every group in it has to come out again.

**An `INFO` may name a group that no longer exists**, and that is correct. Those runs are
carried through untouched: there is no block group to derive a free set from, and dropping
them would delete something the kernel put there deliberately.

The checker's arithmetic is worth following, because it confirms the model rather than
just the result. Before the fix it wanted **81920** bytes free at 30556160 where the tree
said **49152**. The run at 30556160 covers 30556160..30605312. The transaction frees the
old root tree at 30605312 and the old extent tree at 30621696, both one node long, which
extends that run to 30638080 — and 30638080 − 30556160 is exactly 81920.

## The dev tree needs nothing, and that was measured rather than assumed

The dev tree (objectid 4) holds `DEV_EXTENT` items mapping physical device ranges to the
chunks on them — the reverse of the chunk tree's logical-to-physical map. An empty commit
rewrites it, which made it look like a transaction has to maintain it.

Rewriting a block is not the same as changing what it says.
`examples/dev_tree_diff.rs` compares the items either side of both fixture pairs:

```text
before -> control (an empty commit)   6 items, added [] removed [] changed []
before -> after   (one touch)         6 items, added [] removed [] changed []
```

Identical both times. The kernel rewrites the dev tree because a commit copies every tree
it touches and it touches that one; the contents do not move because nothing about the
device layout changed.

A transaction that only relocates metadata within existing chunks therefore needs no dev
tree work at all, and the end-to-end gate confirms it from the other direction: the
filesystem this write path produces passes `btrfs check` and mounts read-write without the
dev tree being touched.

**What would change it** is allocating or freeing a CHUNK — that adds or removes a
`DEV_EXTENT` and moves the chunk tree with it. No fixture does that yet, and the allocator
refuses rather than growing the filesystem, so the case cannot arise today.
