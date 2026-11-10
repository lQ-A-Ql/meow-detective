# Human-code report — 2026-08-28

> **This is analysis only. No code was changed, no branch was created, nothing was
> committed.** Every finding below is a proposal for you to accept, reject or defer.
> The working tree holds this file and nothing else.

**Scope:** the whole crate — `src/` (4,895 production lines across 19 files, test
modules excluded), `tests/` (21 integration binaries + `tests/common/`), `examples/`,
and the design documents the code cites.

**Counts:** 27 findings — **8 High**, **14 Medium**, **5 Low**. 0 fixed. 5 candidate
smells inspected and deliberately not reported (see *Not reported*); 9 more inspected in
the test suite and cleared (see T-section).

The reader is mature and the write path is two days old, so the two halves were read
against different bars. The reader was read for drift; the write path was read for the
three things most likely to be wrong in new code that was measured into existence:
duplication of things the reader already owns, code harder to follow than the format it
encodes, and tests that cannot fail.

The headline is not a bug. It is that **the write path's own documentation describes a
narrower module than the one that now exists** — `render_plan` carries a "What it does
not do" section stating the exact opposite of what its body does, forty lines above the
code that does it. Everything else is smaller.

**The fourth test that cannot fail exists** (T1), and it is not in the write path — it is
the LZO framing test in `compression.rs`, which asserts against the output of the test
module's own frame builder and never calls the decoder. Its production branch has no
other executed coverage anywhere in CI. T2 is the reason why.

Findings are numbered H/M/L in the main section and T in the test section; the numbering
is stable, not contiguous by severity.

---

## Findings

### H1 — `render_plan`'s doc block states the opposite of what the function does

**File:** `src/transaction.rs:370-377` (the doc) vs `src/transaction.rs:445-455` (the code)
**Category:** comments that lie · **Severity:** High
**Coverage:** `tests/render_plan.rs` (4 tests), `tests/transaction_oracle.rs` (5 tests),
`examples/write_transaction.rs` end-to-end. The *code* is well covered; the doc is not
checkable by anything.

The docblock on `render_plan` says:

```rust
/// # What it does not do
///
/// It does not add or remove anything. A relocation is the part of a
/// transaction that moves what is already there; the item changes
/// that record the moves in the extent tree are separate and are not
/// produced here. So a filesystem committed from this alone has a
/// correct tree and an extent tree that still describes the old
/// addresses.
```

Seventy lines later, in the same function:

```rust
if rewrite.owner == objectid::EXTENT_TREE {
    owned = self.apply_records(rewrite.old, owned, plan, generation)?;
}
```

whose own inline comment reads *"Without this the tree is correct and the extent tree
describes a filesystem that no longer exists."* — the doc's claim, restated as the thing
this code exists to prevent. `apply_records` (`:640`) deletes and inserts exactly the
`METADATA_ITEM`s the doc says are "not produced here", and `apply_free_space` (`:768`)
rewrites the free-space tree too.

The module header has the same problem at `src/transaction.rs:30-35`: *"none of that is
implemented … it does not yet produce the item changes that make it true."*

`docs/cow-transaction.md` is the one that is current — its closing section runs the whole
path and reports `btrfs check` clean, which is a result the docblock's world could not
produce. So the fix is to delete these two claims, not to reconcile them.

**Why it matters more than a stale comment usually does.** This is the load-bearing
function of the write path, and the stale claim is not vague — it is a precise, confident
statement about the on-disk state a caller is left with. A reader deciding whether they
must record allocations themselves gets the wrong answer with no reason to doubt it, and
the doc is more prominent than the code that contradicts it.

---

### H2 — `fs.rs` says decompression is not implemented, in the module that decompresses

**File:** `src/fs.rs:25-27` vs `src/fs.rs:37`, `:911-935`
**Category:** comments that lie · **Severity:** High
**Coverage:** `tests/compression_oracle.rs` (3 tests) covers the decompression; nothing
covers the claim.

```rust
//! # What is deliberately refused
//! - **Compressed extents.** Decompression is not implemented. Returning
//!   the raw compressed bytes would look like a successful read of
//!   corrupt data.
```

`src/fs.rs:37` imports `crate::compression::{self, Compression}`; `read_file` at `:911`
matches `Piece::Compressed` and calls `compression::decompress` at `:927`; `Cargo.toml`
carries `miniz_oxide`, `rust-lzo1x` and `ruzstd` with a comment naming one per Btrfs
compression type. Three algorithms are implemented and cross-validated.

This is in the "what is deliberately refused" list — the section a consumer reads to
decide whether this driver is usable for their data. It refuses a capability the crate
has.

---

### H3 — The write path hand-rolls leaf-item parsing that `btree.rs` already does

**Files:** `src/transaction.rs:220-231`, `:266-284`, `:597-602`
**Category:** duplication (reader ↔ write path) + magic numbers · **Severity:** High
**Coverage:** exercised indirectly by `tests/transaction_plan.rs` and
`tests/render_plan.rs`; no test targets the hand-rolled decoding itself.

Three visitors in `transaction.rs` decode leaf items from raw bytes:

```rust
let it = HEADER_SIZE + i * 25;
if it + 25 > block.len() { break; }
let oid = u64::from_le_bytes(block[it..it + 8].try_into().unwrap());
if oid == objectid && block[it + 8] == ROOT_ITEM_KEY { … }
```

and, for the item body:

```rust
let off = HEADER_SIZE
    + u32::from_le_bytes(block[it + 17..it + 21].try_into().unwrap()) as usize;
```

`btree.rs` already owns every one of these: `ITEM_SIZE` (`:97`, the `25`),
`LEAF_DATA_OFFSET` (`:89`), `Item::parse` (`:313`, which decodes the key at `+0..17`, the
offset at `+17`, the size at `+21`), and `TreeBlock::item_data` (`:519`, which is exactly
`LEAF_DATA_OFFSET + item.offset` with checked arithmetic). `transaction.rs` already
imports `HEADER_SIZE` and `header_offsets` from that module — it stopped one import
short and wrote the rest out longhand, with `25` as a bare literal in four places.

**The root cause is one line.** `for_each_tree_block` (`:301`) parses the block, then
hands the *visitor* only `block.bytes()`:

```rust
visit(at, block.bytes(), level, parent);
```

It has a fully parsed `TreeBlock` in hand and passes raw bytes, so every visitor
re-parses. `render_plan` — which gets its block from `read_tree_block` instead — uses
`block.body.items()` and `block.item_data(item)` and is markedly easier to read for it.
Changing the `BlockVisitor` type to carry `&TreeBlock` would delete all three hand-rolled
loops.

**A second-order consequence worth naming.** Both hand-rolled loops skip silently on a
short read — `break` at `:223`, `continue` at `:269`, and the guarded
`if off + ROOT_ITEM_BYTENR + 8 <= block.len()` at `:274`. A `ROOT_ITEM` whose body lands
near the end of a block is therefore not discovered by `placements()`, that tree's blocks
land in no placement map, and `plan_transaction` reports them as *"not reachable from any
tree"* — a refusal whose message points away from the cause. `TreeBlock::item_data`
returns `Option` for the same condition, which at least puts the decision where the
caller can see it.

---

### H4 — The free-run merge in `apply_free_space` is a verbatim copy of `merge_adjacent`

**Files:** `src/transaction.rs:837-843` vs `src/block_group.rs:565-574`
**Category:** duplication (reader ↔ write path) · **Severity:** High
**Coverage:** the original has a unit test (`block_group.rs:586`,
`adjacent_runs_merge_and_separated_ones_do_not`). The copy has none — it is reached only
through the free-space oracle.

```rust
// transaction.rs:837
let mut merged: Vec<FreeExtent> = Vec::with_capacity(free.len());
for run in free {
    match merged.last_mut() {
        Some(prev) if prev.end() == run.start => prev.len += run.len,
        _ => merged.push(run),
    }
}
```

```rust
// block_group.rs:565
fn merge_adjacent(runs: Vec<FreeExtent>) -> Vec<FreeExtent> {
    let mut out: Vec<FreeExtent> = Vec::with_capacity(runs.len());
    for run in runs {
        match out.last_mut() {
            Some(prev) if prev.end() == run.start => prev.len += run.len,
            _ => out.push(run),
        }
    }
    out
}
```

Identical modulo two variable names, on the same `FreeExtent` type, in the same crate.
`merge_adjacent` is private to `block_group.rs`; making it `pub(crate)` and calling it is
a two-line change.

This is the one duplication where the two copies are *guaranteed* to need to agree:
`block_group::cached_free_extents` merges what the kernel wrote and
`transaction::apply_free_space` merges what we are about to write, and
`tests/free_space_oracle.rs` compares the two. If they ever drift the oracle reports a
mismatch and the cause will be that the same rule was written twice.

---

### H5 — `insert`'s refusal message contradicts the `split` function 50 lines below it

**Files:** `src/leaf_edit.rs:117-122`, pinned by `src/leaf_edit.rs:286` and
`tests/leaf_edit_oracle.rs:254`
**Category:** comments that lie · **Severity:** High
**Coverage:** two tests assert the message — and therefore assert the false claim.

```rust
return Err(Error::UnsupportedFeature(format!(
    "the item does not fit: {} items need {needed} bytes and a leaf holds \
     {capacity}. Splitting a leaf is not implemented — where the kernel puts the \
     boundary is a policy this has not measured.",
    out.len()
)));
```

Both halves are false as of the current tree:

- **"Splitting a leaf is not implemented"** — `leaf_edit::split` is at `:170`, fifty
  lines below, with `tests/split_oracle.rs` (4 tests) checking it against leaves the
  kernel really split.
- **"a policy this has not measured"** — the module docs directly above (`:23-38`) print
  three measured splits, and `docs/cow-transaction.md` devotes a section to the
  measurement and to why the boundary is deliberately *not* copied.

This is the crate's house style working against itself: the message is long and specific
precisely so a caller knows what to do next, and what it tells them to do is give up on
something that exists.

**The pin matters.** `src/leaf_edit.rs:286` and `tests/leaf_edit_oracle.rs:254` both
assert `err.to_string().contains("Splitting a leaf is not implemented")`, so correcting
the message breaks two tests. Whatever replaces it, those assertions should test the
*condition* (an item that will not fit is refused) rather than the sentence.

`src/tree_write.rs` has a milder version of the same disagreement inside one function:
the doc at `:137-139` says splitting is *"the caller's decision, not this function's"* —
correct — and the error it raises at `:163-167` says it *"is not implemented"*.

---

### H6 — A short `FREE_SPACE_INFO` item silently keeps a stale extent count

**File:** `src/transaction.rs:854-857`
**Category:** speculative/defensive code that hides a failure + magic number ·
**Severity:** High
**Coverage:** `tests/free_space_oracle.rs` (4 tests) exercises the happy path; nothing
exercises the short-item branch, and nothing can, because it produces no observable
signal.

```rust
let mut info = item.clone();
if info.data.len() >= 4 {
    info.data[0..4].copy_from_slice(&(runs.len() as u32).to_le_bytes());
}
out.push(info);
```

If the guard fails, the `FREE_SPACE_INFO` item is pushed with the count it had before,
while the `FREE_SPACE_EXTENT` items after it are rewritten from the new free set. That is
precisely the state this function's own docblock says it exists to avoid — *"A leaf left
saying where things used to be is what `btrfs check` reports as 'cache appears valid but
isn't'"* — reached by the one path that does not report anything.

Every other refusal in this module is loud and names the missing piece. This one is a
silent `if`.

Two smaller things in the same three lines: `0..4` is a bare offset where
`block_group::free_space_info::EXTENT_COUNT` (`block_group.rs:60`) already names it, and
`>= 4` is a bare length where `free_space_info::SIZE` (`:64`) already names the whole
item. `block_group.rs` reads that field through the named offsets; `transaction.rs`
writes it through raw ones.

---

### M7 — Six declarations of `ROOT_ITEM_KEY`, four of `root_item::BYTENR`

**Files:** see table · **Category:** magic numbers not centralised · **Severity:** Medium
**Coverage:** every user is covered; the scatter itself is not the kind of thing a test
sees.

`chunk.rs` already holds `pub mod key_type` and `pub mod objectid` — the crate's own
answer to "where do format constants live" — and both are used across module boundaries.
The write path did not extend them; it declared its own.

| value | canonical home | other declarations |
|---|---|---|
| `ROOT_ITEM_KEY = 132` | *(absent from `chunk::key_type`)* | `fs.rs:58` (pub), `transaction.rs:214`, `transaction.rs:239`, `transaction.rs:351`, `write.rs:54`, `subvol.rs:51`, `block_group.rs:460`; bare `132` with a comment at `dir.rs:385` |
| `root_item::BYTENR = 176` | `fs.rs:70` (`pub(crate) mod root_item`) | `subvol.rs:78`, `transaction.rs:336`, and `transaction.rs:241` under the different name `ROOT_ITEM_BYTENR` |
| `root_item::GENERATION = 160` | — | `subvol.rs:80`, `transaction.rs:345` |
| `root_item::LEVEL = 238` | `fs.rs:73` | `transaction.rs:347` |
| `FREE_SPACE_{INFO,EXTENT,BITMAP}_KEY` | `chunk::key_type` (`:85-91`) | `transaction.rs:707-709` |
| `EXTENT_ITEM = 168` | `chunk::key_type:69` | `write.rs:52` |
| `EXTENT_TREE = 2` | `chunk::objectid:103` | `write.rs:50` |
| `FS_TREE = 5` | `chunk::objectid:109` | `fs.rs:48`, `subvol.rs:59` |
| `btrfs_item` size `25` | `btree::ITEM_SIZE:97` | bare literal, `transaction.rs:222`, `:223`, `:268`, `:269` |

`transaction.rs` alone declares `ROOT_ITEM_KEY` three times in one file — twice inside
function bodies, once at module scope — with the module-scope copy sitting 112 lines below
the second function-local one.

**The `root_item` case is the one with teeth.** Three modules each define offsets into
`btrfs_root_item`, and they do not carry the same knowledge. `transaction.rs:337-345`
records a measurement the other two lack:

> AT 160, after the embedded `btrfs_inode_item`. Offset 16 is inside that inode and holds
> something else entirely — a ROOT_ITEM whose generation was written there leaves the real
> field stale, and the kernel refuses the tree it names with "parent transid verify
> failed". Which is exactly what `btrfs check` said before this was measured.

A reader who lands on `fs.rs:70` or `subvol.rs:78` first — both are older and more
discoverable — gets the offsets without the warning that cost a debugging round to learn.
Merging the three into one module is how that warning stops being findable only by luck.

---

### M8 — `apply_free_space` is 107 lines and four jobs

**File:** `src/transaction.rs:768-874` · **Category:** god function · **Severity:** Medium
**Coverage:** `tests/free_space_oracle.rs` (4), `tests/transaction_oracle.rs` (5).

The longest function in the write path, and the one where the code is harder to follow
than the thing it encodes — which is the bar this crate otherwise clears. It does, in one
body: refuse bitmaps; walk the item list with a manual `i`/`j` cursor to find each
group's run; decide per group whether the plan touched it; recompute the free set;
merge adjacent runs; carve out allocations; patch the info item's count; and emit.

The manual cursor is the densest part:

```rust
let mut j = i + 1;
while j < items.len()
    && items[j].key.key_type != FREE_SPACE_INFO_KEY
    && items[j].key.objectid < end
{ j += 1; }
```

`i..j` is "one `FREE_SPACE_INFO` and the extents belonging to it" — the exact structure
the docblock above describes in prose and the exact structure `docs/cow-transaction.md`
prints as a diagram. A `fn runs_by_group(items) -> Vec<(&OwnedItem, &[OwnedItem])>` would
put the shape the measurement established into the type, and leave the body as a `match`
over three cases per group.

The `match (touched, group)` at `:821` is good and should survive any extraction — it is
the place the "an `INFO` may name a group that no longer exists" finding lives, and it
reads clearly.

---

### M9 — `render_plan` is 106 lines across four abstraction levels

**File:** `src/transaction.rs:385-490` · **Category:** god function · **Severity:** Medium
**Coverage:** `tests/render_plan.rs` (4).

Address remapping, node re-pointing, three owner-specific leaf rewrites, and raw
`ROOT_ITEM` byte-patching, all in one body. The three owner branches (`:445`, `:453`,
`:458`) are each a named concern with a comment already naming it — `apply_records` and
`apply_free_space` are already extracted, and the third (`ROOT_ITEM` relocation, `:458-477`)
is the only one still inline, and the only one that manipulates raw item bytes at this
level.

Extracting it as `relocate_root_items(&mut owned, &moved, generation)` would make the
three branches read the same way and leave `render_plan` as what its doc says it is: a
dispatcher over "leaf or node, and which tree".

---

### M10 — `leaf_edit::insert_or_split` has no callers and no tests

**File:** `src/leaf_edit.rs:195-222` · **Category:** speculative code · **Severity:** Medium
**Coverage:** **none.** Not called from `src/`, `tests/` or `examples/`; the split branch
has never executed.

It is the only function that does the thing the write path actually needs — insert, and
divide the leaf if the item will not fit — and `transaction::apply_records` (`:693`) calls
plain `insert` instead, which is the function that raises the H5 refusal.

Two consequences:

- the crate's answer to "what happens when an extent-tree leaf fills up" is written, is
  correct as far as anyone can tell, and is unreachable;
- `insert_or_split:205-218` re-derives `insert`'s binary search and splice
  (`:97-111`) verbatim, so the duplicate exists to serve a caller that does not exist.

This is worth a decision rather than a refactor: either wire it into `apply_records` (and
then H5's message becomes true only for `insert`, which is a narrower and defensible
claim), or take it out until the caller exists. Leaving an untested split path in a crate
whose whole method is "measure it, then check it" is the option that fits least.

---

### M11 — Four independent implementations of "find a tree's root"

**Files:** `src/block_group.rs:459-481` (`tree_root`), `src/write.rs:250-273`
(`extent_tree_root`), `src/fs.rs:420-438` (inline in `open_pool`),
`src/transaction.rs:211-234` (`root_item_leaf`)
**Category:** duplication · **Severity:** Medium
**Coverage:** all four are exercised; none has a test that distinguishes it from the
others.

`block_group::tree_root(objectid)` is the general one and is `pub(crate)`.
`write::extent_tree_root()` is `tree_root(objectid::EXTENT_TREE)` written out again, down
to its own `EXTENT_TREE_OBJECTID`, `ROOT_ITEM_KEY` and `Tree::from_superblock` setup.
`open_pool`'s copy is the FS_TREE case, and predates the general one.

`transaction::root_item_leaf` is a genuinely different question — *which leaf holds* the
`ROOT_ITEM`, not what it points at — so it is not the same function. But it scans the
same items for the same key type, by hand (see H3), and would be three lines if the
scan were shared.

Two of the three plain duplicates are in the older reader, so this is drift rather than a
new mistake; it is listed here because the write path now depends on the general one and
the drift makes it harder to see that `tree_root` is the answer.

---

### M12 — `mine` reads as a set-membership test and is a full B-tree traversal

**File:** `src/transaction.rs:655-657`, used at `:664` and `:680`
**Category:** misleading name / dense logic · **Severity:** Medium
**Coverage:** `tests/render_plan.rs`, `tests/transaction_oracle.rs`.

```rust
let mine =
    |at: u64| -> Result<bool> { Ok(self.leaves_holding(root, &[at])?.contains(&leaf)) };
```

`leaves_holding` (`:586`) calls `for_each_tree_block`, which walks and checksums every
block of the extent tree. `mine` is then called inside two loops over `plan.rewrites`, so
a plan of *n* rewrites walks the extent tree *2n* times. On the fixtures that is fast
enough not to notice; the readability problem is that nothing at the call site suggests
the cost, and the name suggests the opposite.

The set `leaves_holding(root, &all_addresses)` is already computed once per round inside
`plan_transaction_closed` (`:547`). Hoisting the map out of the loops — or passing it in —
makes both the cost and the invariant ("every leaf any address falls into is in the plan",
which the docblock at `:628-632` asserts) visible in one place.

---

### M13 — The read-closure + `Tree` construction boilerplate appears eleven times

**Files:** `block_group.rs:144`, `:251`, `:304`, `:408`, `:442`, `:461`;
`transaction.rs:302`; `write.rs:217`, `:251`; `fs.rs:603`, `:633`
**Category:** duplication · **Severity:** Medium
**Coverage:** incidental.

```rust
let read = |logical: u64, buf: &mut [u8]| -> Result<()> {
    Self::read_logical_pool(&self.device, &self.devices, &self.map, logical, buf)
};
let tree = Tree::from_superblock(&self.sb, &read);
```

Four lines, eleven times, character-identical in ten of them (`fs.rs:603` differs only in
being followed by a method call). Well past the three-instance threshold.

The borrow checker is why this has not already been factored — the closure borrows three
fields and the `Tree` borrows the closure, so a `fn tree(&self) -> Tree<'_>` does not
type-check directly. A small owned struct holding the three `Arc`/map references, with a
`fn tree(&self) -> Tree<'_>` on *it*, does. Worth doing once; not worth doing badly.

---

### M14 — `lib.rs`'s status and architecture sections describe an earlier crate

**File:** `src/lib.rs:31`, `:33-45` · **Category:** comments that lie · **Severity:** Medium
**Coverage:** none possible.

```rust
//! Steps 1 and 2 are what this crate implements today.
```

All four bootstrap steps are implemented — `fs::open_pool` performs step 4 at `:420` — and
the crate reads files, lists directories, resolves subvolumes, decompresses three
formats, and commits transactions.

The `Architecture:` list below it names three modules (`error`, `superblock`, `chunk`) of
the nineteen in the crate. `block_group`, `btree`, `capi`, `commit`, `compression`, `dir`,
`extent_write`, `fs`, `inode`, `leaf_edit`, `subvol`, `super_write`, `transaction`,
`tree_write` and `write` are all absent — including every module of the write path.

This is the crate's front door and the first thing `cargo doc` renders.

---

### M15 — `examples/write_transaction.rs` invalidates the free-space tree for a reason that no longer holds

**File:** `examples/write_transaction.rs:59-61` · **Category:** comments that lie ·
**Severity:** Medium
**Coverage:** the example is the end-to-end gate `docs/cow-transaction.md` reports on.

```rust
// This transaction moves blocks and does not maintain the
// free-space tree, so the cache must be marked untrusted.
invalidate_free_space_tree: true,
```

`render_plan` maintains the free-space tree — `apply_free_space` at `transaction.rs:453`.
`docs/cow-transaction.md` records that as landed under *"The free-space tree, second
attempt"*, several sections after the passage this comment paraphrases.

Whether the flag should still be `true` is a real question and not one this report can
settle: `apply_free_space` refuses bitmaps and rewrites extents, so "maintained" may still
be "maintained for the cases we handle". But the stated reason is now false, and it is the
reason a reader would use to decide whether to copy this into real code.

---

### M16 — `le32` / `le64` / `items_of` are copied across eight test binaries that share a helper module

**Files:** `le64` in `tests/{extent_write,leaf_edit,node_write,render_plan,split,super_write,transaction,tree_write}_oracle.rs`; `le32` in five of those; `items_of` in `leaf_edit_oracle.rs:55`, `render_plan.rs:45`, `split_oracle.rs:38`, `tree_write_oracle.rs:64`
**Category:** duplication · **Severity:** Medium
**Coverage:** n/a — this is the test code.

`tests/common/mod.rs` exists and says exactly what it is for:

> Rust builds each file in `tests/` as its own binary, so anything two of them need has to
> live here and be pulled in with `mod common;`.

Three files use it. Eight re-declare `le64`.

**This does not compromise the oracles' independence, and that is worth stating**, because
it is the reason not to fix this the obvious way. These tests decode leaves by hand
*deliberately* — an oracle that read a leaf through `btree::TreeBlock` would be checking
the writer against the reader instead of against the disk. Moving the hand-rolled decoder
into `tests/common/` keeps it independent of `src/` while removing seven copies. Moving it
into `src/` would not.

The four `items_of` copies return four different types (`Vec<OwnedItem>`,
`Option<Vec<OwnedItem>>`, `Vec<(DiskKey, Vec<u8>)>`, `Vec<(DiskKey, Range<usize>)>`), so
this is a small design job rather than a copy-paste.

---

### L17 — A parsed flag is masked and discarded

**File:** `src/block_group.rs:381`, `:388` · **Category:** dead code · **Severity:** Low
**Coverage:** `tests/free_space_oracle.rs`.

```rust
let Some((_count, info_flags)) = info else { … };
let _ = info_flags & USING_BITMAPS;
```

`cached_free_extents` parses `FREE_SPACE_INFO`'s two fields, then discards both: the count
into `_count`, the flags into a masked expression bound to `_`. `USING_BITMAPS`
(`block_group.rs:68`) has no other use in the crate.

The tuple's only remaining purpose is the `else` branch — proving an `INFO` item was seen —
which a `bool` would do more honestly. And the discarded `_count` is the same field
`transaction.rs:856` writes back (H6): the reader parses it and ignores it, the writer
writes it without checking it, so nothing in the crate ever compares the two.

Either state why the bitmap flag is read and not acted on — the module docs elsewhere are
excellent about this — or drop the parse and keep the presence check.

---

### L18 — `#[allow(dead_code)]` on `root_item::LEVEL` is now false

**File:** `src/fs.rs:71-73` · **Category:** stale suppression · **Severity:** Low

```rust
pub(crate) mod root_item {
    pub const BYTENR: usize = 176;
    #[allow(dead_code)]
    pub const LEVEL: usize = 238;
}
```

`LEVEL` is read at `fs.rs:431`, `block_group.rs:469` and `write.rs:259`. The suppression
predates those callers. (The 2026-08-25 review's L5 raised the *other* `#[allow]` in this
file, on `file_extent`, which is still correct and still explained.)

---

### L19 — `commit.rs` says it is waiting for a piece that exists

**File:** `src/commit.rs:44-51` · **Category:** comments that lie · **Severity:** Low

> Deciding which blocks a change produces, allocating them, and recording those
> allocations in the extent tree all happen first … **That is the piece this is waiting
> on**, not a shortcut taken here.

All three exist: `plan_transaction_closed`, `next_free_block`, `apply_records`. The
paragraph's substance — that `commit` writes what it is given and decides nothing — is
still exactly right; only "waiting on" is stale. A one-clause edit.

---

### L20 — `carve` sits at the bottom of `transaction.rs`, a hundred lines from its caller

**File:** `src/transaction.rs:877-902`, called at `:849` · **Category:** organisation ·
**Severity:** Low

A free-file function after the last `impl` block, with a one-line doc, doing set
subtraction on `block_group::FreeExtent`. Its sibling operations — `merge_adjacent`,
`gaps`, `usable_in`, `place_in_run` — all live in `block_group.rs` next to the type.
`carve` is the odd one out, and `block_group::gaps` (`:212`) is the closest thing to it in
the crate.

Small, but it is the piece a reader of `apply_free_space` most needs and the piece placed
furthest from them.

---

## Findings — tests that cannot fail

The brief said a fourth was plausible after three were caught by mutation testing. There
is one, and finding it turned up a second problem that explains why nothing caught it.

Nine other candidates were inspected and cleared — they are listed at the end of this
section, because knowing what was checked and found sound is as useful as the list of
what was not.

---

### T1 — The LZO sector-padding test asserts against the test module's own frame builder

**File:** `src/compression.rs:346-371`
(`lzo_segment_headers_do_not_straddle_a_sector`)
**Category:** tautology · **Severity:** High · **Confidence:** certain

```rust
let framed = lzo_frame(&[first.clone(), second.clone()], sectorsize);

// The second segment must begin at the sector boundary.
assert_eq!(read_u32(&framed, sectorsize).unwrap() as usize, second.len());
assert_eq!(&framed[sectorsize + LZO_LEN..sectorsize + LZO_LEN + 8], &second[..]);
```

`lzo_frame` is the test module's own helper, at `src/compression.rs:301`, and it is the
thing that implements the padding:

```rust
if sectorsize - (out.len() % sectorsize) < LZO_LEN {
    let pad = sectorsize - (out.len() % sectorsize);
    out.resize(out.len() + pad, 0);
}
```

The test builds a frame using the padding rule and then asserts the padding is there.
**It never calls `decompress`.** The production branch it is named for —
`src/compression.rs:193-198`:

```rust
let room = sectorsize - (at % sectorsize);
if room < LZO_LEN { at += room; continue; }
```

can be deleted outright and this test stays green. The only crate code it touches is
`read_u32` and the `LZO_LEN` constant.

What makes this stand out rather than being an ordinary near-miss is that every
neighbouring test in the same module does it right: `lzo_declaring_more_than_it_holds_is_refused`,
`lzo_shorter_than_its_header_is_refused`, `lzo_with_a_zero_length_segment_is_refused` and
`lzo_with_a_segment_past_the_end_is_refused` all call `decompress` and assert on its
error. This one builds the input and stops.

**And nothing else covers that branch.** `lzo_with_a_zero_length_segment_is_refused`
frames a single empty segment and never reaches the skip. The only real coverage is
`tests/compression_oracle.rs` against `.vm-share/btrfs-comp-lzo.img` — which CI never
runs (T2). So the rule the module docs call the reason `decompress_lzo` exists at all has
**zero executed coverage in CI**, and its unit test cannot fail.

The fix is one line: build `framed`, then assert on
`decompress(Compression::Lzo, &framed, ram, sectorsize)`.

---

### T2 — Six oracle suites report success on every push and pull request

**Files:** `.github/workflows/ci.yml:19-46` (the `test` job), `:79-270` (`kernel-gate`)
**Category:** fixture-missing early return, at scale · **Severity:** High ·
**Confidence:** certain

Every test in these six binaries is `if fixture missing { eprintln!(…); return; }`, and
`.vm-share/` is gitignored:

| suite | tests | fixtures it needs | built by `build-fixtures-native.sh`? |
|---|---|---|---|
| `tests/capi.rs` | 25 | `btrfs-rich`, geometry matrix | yes |
| `tests/fs_oracle.rs` | 13 | geometry matrix | yes |
| `tests/btree_oracle.rs` | 4 | geometry matrix | yes |
| `tests/fstree_oracle.rs` | 2 | geometry matrix | yes |
| `tests/compression_oracle.rs` | 3 | `btrfs-comp-{zlib,lzo,zstd}` | **no** |
| `tests/write_oracle.rs` | 3 | `btrfs-nodatacow`, `btrfs-write`, `btrfs-ro`, `btrfs-cow-refused` | **no** |

The `test` job runs `cargo test --release` (`ci.yml:46`) — every target, **no fixtures**.
The `kernel-gate` job *does* build fixtures (`ci.yml:116`) but then invokes fourteen test
targets one at a time by name (`--test super_write_oracle`, `--test subvol_oracle`, …) and
**names none of these six**. So on every push and PR, 50 tests print a skip line and pass.

`ci.yml` diagnoses this exact failure mode in its own comments, for the suites it *did*
remember:

> without fixtures they find nothing and report success — which is what the leaf oracle
> did until this step existed

**Two mitigations, and the limit of each.** `release.yml:63` runs
`build-fixtures-native.sh` and then `cargo test --all-targets` (`:66`), so on a release
tag the first four suites do run for real. That is the right gate at the wrong end of the
pipeline — it catches a regression when you tag, not when you merge.

The last two are not covered even there. `btrfs-comp-*` and `btrfs-nodatacow`/`btrfs-write`
are built only by `scripts/vm-build-fixtures.sh`, the Vagrant path, which no workflow
runs. **`tests/compression_oracle.rs` and `tests/write_oracle.rs` have never executed an
assertion in CI.** That is three compression algorithms and the entire `nodatacow`
in-place write path.

`tests/write_oracle.rs:138-141` compounds it independently of fixtures:

```rust
let Some(out) = vm_run(&script) else {
    eprintln!("oracle VM unavailable — skipping verification");
    return;
};
```

Everything `an_in_place_write_survives_the_kernel_and_the_checker` promises — the kernel's
SHA read-back at `:148`, `btrfs check`'s exit status at `:161` — is downstream of that
return, and `vm_run` returns `None` on any non-zero exit including "VM is not up". The
crate's only kernel-judged write test is green everywhere except a dev box with both the
fixture and a running VM.

Adding the six names to `kernel-gate` fixes four of them. The other two need
`build-fixtures-native.sh` to learn to build compression and nodatacow images, which is a
larger job and the one worth scheduling.

---

### T3 — Two error-message tests assert less than their names claim

**Files:** `src/error.rs:186-196`, `src/error.rs:159-171`
**Category:** assertion weaker than its name · **Severity:** Medium · **Confidence:** certain

```rust
#[test]
fn identity_mismatch_names_both_addresses() {
    let e = Error::BlockIdentityMismatch {
        what: "tree block", expected: 0x1000, found: 0x2000,
    };
    let s = e.to_string();
    assert!(s.contains("tree block"), "structure missing from: {s}");
    assert!(!s.is_empty());
}
```

Neither address is checked; `4096`, `8192`, `1000` and `2000` appear nowhere in the test.
The Display arm (`:118-122`) could be reduced to `write!(f, "{what} identity mismatch")`
and this passes — while the test's name is exactly the claim it does not make. The second
assertion is also redundant after the first.

`not_btrfs_shows_the_magic_it_found` (`:159`) is the same shape: Display is
`"not a Btrfs volume (magic bytes {magic:02x?})"`, and both assertions —
`!s.is_empty()` and `s.to_lowercase().contains("btrfs")` — are satisfied by the literal
word in the format string. Dropping `{magic:02x?}` leaves it green. (The magic used,
`NOTABTRF`, lowercases to `notabtrf`, so it is not even accidentally covering the payload.)

**The correct pattern is one test away, in the same module.**
`checksum_mismatch_names_the_structure_and_offset` (`:172`) does it properly:

```rust
assert!(s.contains("65536") || s.contains("10000"), "offset missing: {s}");
```

This matters more here than it would in most crates, because the long, specific error
message *is* the interface — H5 above is a case of one of them going false and no test
noticing.

---

### T4 — `transaction_oracle` documents a backstop it does not implement

**File:** `tests/transaction_oracle.rs:289-323`
(`the_change_in_usage_is_the_change_in_recorded_blocks`), doc at `:98-108`
**Category:** guarded skip · **Severity:** Medium · **Confidence:** certain that the
guard is absent; the test is not vacuous on today's fixtures

The module doc states the requirement precisely:

> A pair with no transaction in it is not a failure… The tests below skip those **and
> require that something, somewhere, did commit** — otherwise a fixture builder that
> silently stopped producing transactions would read as a pass.

That requirement is not implemented. At `:307`:

```rust
if !committed(before, after) {
    eprintln!("{what}: no commit happened on this run — nothing to check");
    continue;
}
```

There is no counter and no assertion after the loop. `committed()` has exactly one call
site. If neither pair commits, the test runs zero assertions and passes — which is the
scenario the doc was written to prevent, and which `docs/cow-transaction.md` records as
already having happened on the CI runner for the control pair.

On the local fixtures both pairs do commit (generation 8→9 control, 8→10 after), so this
is a missing backstop rather than a currently-vacuous test. The sibling suites already
have the pattern: `extent_write_oracle`, `free_space_oracle`, `node_write_oracle` and
`leaf_edit_oracle` all end with a terminal `assert!(checked > 0)`-style guard.

---

### T5 — Silent `else { return; }` skips with no diagnostic line

**Files:** `tests/transaction_plan.rs:113`, `:168`, `:253`, `:314`, `:356`, `:360`;
`tests/render_plan.rs:223`, `:284`; `tests/tree_write_oracle.rs:431`, `:478`, `:481`,
`:487`, `:540`, `:582`; `tests/leaf_edit_oracle.rs:183`, `:235`
**Category:** guarded skip · **Severity:** Medium · **Confidence:** certain

These differ from the crate's documented skip idiom — `eprintln!("no fixture — skipping")`
then `return` — in emitting nothing. A skip is then indistinguishable from a pass in the
CI log, which is the property that lets a silent regression sit.

The mechanism matters more than the count. `leaves(img)` and `nodes(img)` return `None` on
**any** mount or read failure, not only on a missing fixture. So a regression that breaks
mounting turns four `tree_write_oracle` refusal tests and two `leaf_edit_oracle` tests from
failures into passes.

Two are worth naming individually:

- `tests/transaction_plan.rs:360` — `let Ok(plan) = fs.plan_transaction_closed(&[fs_root], 8) else { return; };`
  A planner that returns `Err` makes `a_closed_plan_places_every_block_somewhere_free` pass
  silently, skipping the distinctness, no-overwrite, alignment and freeness checks. The
  sibling test at `:320` uses `.expect("the plan should settle…")` for the identical call,
  so the strict treatment is clearly the intent.
- `tests/render_plan.rs:223`, `:284` — these guard
  `a_root_item_for_a_tree_that_moved_names_the_new_address` and
  `a_node_follows_a_child_that_moved`, which the file's own docs call the characteristic
  copy-on-write bug.

---

### T6 — `the_kernels_own_boundary_is_recorded` can execute zero assertions

**File:** `tests/split_oracle.rs:241-270`
**Category:** vacuous loop · **Severity:** Medium · **Confidence:** likely

```rust
for pair in ["", "-vary"] {
    let Some(real) = real_split(pair) else { continue };
    …assert!(…)
}
if seen == 0 { eprintln!(…) }
```

`eprintln!`, not `assert!`. With either split fixture pair absent the test executes no
assertion and reports success. The counter exists — it is simply reported rather than
asserted, one character short of the backstop the other suites have.

---

### T7 — The decoded-length check is tested through miniz's limit, not its own

**File:** `src/compression.rs:280` (`a_stream_longer_than_the_item_records_is_refused`) vs
`src/compression.rs:103-108`
**Category:** assertion weaker than its name · **Severity:** Low · **Confidence:** likely

`decompress`'s doc says *"The decoded length is checked rather than trusted."* The test
asserts `decompress(Zlib, &packed, 100, 4096).is_err()` — and the refusal comes from
`decompress_to_vec_zlib_with_limit` (`:115`), not from the crate's check. The test's own
inline comment already admits this.

The crate's check at `:103-108` is unreachable for the three algorithms that have their own
bound: Zlib (miniz's limit), Zstd (`.take(ram_bytes)`, `:132`), and — in practice — LZO
(`want = sectorsize.min(ram_bytes - out.len())`, `:220`, inside a loop conditioned on
`out.len() < ram_bytes`). It is genuinely reachable only for `Compression::None`, whose arm
is `input.to_vec()` (`:92`) with no bound at all, and which no test exercises with
`input.len() > ram_bytes`.

So the check is not dead — it is the only thing standing between an over-long uncompressed
extent and the caller's slice — and it is the one case nothing tests. One test with
`Compression::None` closes it.

---

### Inspected and cleared

Nine candidates that looked vacuous and are not. Recorded so they are not re-litigated.

| candidate | why it holds |
|---|---|
| `tests/super_write_oracle.rs:207-213` — `for slot in &changed { assert!(expected.contains(slot)) }`, no non-empty guard, one direction only | The seven `.vm-share/btrfs-commit-*.super` fixtures were decoded: every pair yields `changed = [2,3]` or `[0,1]` against a 2-element `expected`, so it is a real 2-of-4 constraint, and mutations of `backup_slot`'s modulus or constants do fail it. The *name* over-promises; the test does not. |
| `tests/render_plan.rs:75` — `let Some(b) = get(at) else { continue };` | Guarded downstream by `assert!(!was.is_empty())` (`:140`) and the `now.len() == was.len()` comparison. |
| `tests/extent_write_oracle.rs:144` | Terminal `assert!(checked > 0)`. |
| `tests/free_space_oracle.rs:183`, `:284` | Terminal `total > 100` guard. |
| `tests/node_write_oracle.rs:271`, `:277` | Terminal `images_with_nodes > 0` guard. |
| `tests/leaf_edit_oracle.rs:160` | Terminal `exact > 0` guard. |
| `src/error.rs:284` — `implements_std_error` | Compile-time-only assertion; that is the idiom's purpose, not a defect. |
| `tests/bootstrap_chain.rs` | Not fixture-gated at all — synthetic, runs everywhere. |
| Unit test modules in `btree`, `chunk`, `superblock`, `inode`, `dir`, `block_group`, `leaf_edit`, `tree_write`, `subvol`, `extent_write`, `super_write` | No vacuous loops, unguarded skips or self-comparisons. Each module doc states the hand-built-fixture limitation honestly. |

The round-trip question was asked of every encoder test and answered in the crate's favour:
`tree_write_oracle`, `node_write_oracle` and `leaf_edit_oracle` all rebuild leaves the
kernel wrote and compare bytes, rather than encoding and decoding with this crate on both
sides.

---

---

## Not reported

Five things were inspected and deliberately left off the list.

| candidate | why not |
|---|---|
| Superblock backup ring not filled | Documented as a deliberate gap in `super_write.rs:162-173`, and documented *well* — it names the consequence (`btrfs rescue` recovers only as far back as the kernel's last commit). Code and doc agree. |
| Leaf splitting does not imitate the kernel's boundary | Documented in `leaf_edit.rs:12-51` and in `docs/cow-transaction.md`, with the reasoning for why byte-identity is the wrong bar here. Code and doc agree — see H5, which is about a *different* message that contradicts this one. |
| RAID5/6 refused | `superblock.rs:287` and the chunk mapper refuse it explicitly, with a stated reason. |
| `FREE_SPACE_BITMAP` refused in the write path | `transaction.rs:762-767` and `:781-785`. The refusal is loud, names the missing work, and matches the docs. |
| `tree_write::stamp_checksum` vs `super_write::stamp_checksum` | Two instances, below the three-instance extraction rule, and the difference (one bounds the digest at `SUPERBLOCK_SIZE`, the other runs to the end of the block) is meaningful rather than incidental. |

Density in on-disk offset handling was judged case by case rather than counted.
`tree_write::build_leaf` (`:186-200`), `super_write::apply` (`:187-221`) and
`extent_write::body` (`:105-116`) are all dense and all explained by a comment naming the
non-obvious constraint — the item offset being header-relative, the checksum not covering
itself, the level living in the key. None is reported. The instances reported above are
reported because the density is *not* explained (H3's `i * 25`, H6's `0..4`) or because it
encodes something simpler than it looks (M8's `i`/`j` cursor).

---

## Previously reported, still open

From `docs/code-quality-review-2026-08-25.md`, three days ago. Nothing there has been
actioned; the counts below are current.

| item | status |
|---|---|
| **H1** — split checksum code out of `superblock.rs` into `checksum.rs` | Open. `superblock.rs` is still 946 production lines and still holds all four checksum implementations. |
| **M2** — deep nesting in `chunk.rs` / `btree.rs` | Open. |
| **M3** — nine functions of 60+ lines | Open, and now **sixteen** functions of 55+ lines. The three additions are all in the new write path: `transaction::render_plan` (106), `transaction::apply_free_space` (107), `transaction::apply_records` (64). `block_group::cached_free_extents` (95) and `fs::open_pool` (99) were also above the old threshold and not listed then. |
| **M4** — `capi.rs` size, noted for completeness | Still noted, still no action needed. |
| **L5** — `#[allow(dead_code)]` on `file_extent` sits below its explanation | Open. See also L18 above, which is a *different* `#[allow]` in the same file that has since become incorrect. |

The prior review's closing observation — *"No duplication at all. Zero repeated eight-line
blocks across 4,989 lines"* — was true of the reader and is no longer true of the crate.
H3, H4, M11, M13 and M16 are all duplication, and four of the five involve the write path
re-deriving something the reader already owns. That is the expected shape of two days of
fast work against a mature codebase, and it is the cheapest thing on this list to fix.

---

## Test results

No changes were made, so there is no before/after. This is the baseline as observed.

| measure | value |
|---|---|
| `cargo test --lib` | **171 passed, 0 failed, 0 ignored** (0.01s) |
| `#[test]` functions, total | 283 — 171 in `src/`, 112 across 21 integration binaries |
| Integration suites | 21, most gated on `.vm-share/` fixtures built by the Vagrant/oracle scripts |
| Of those, run with fixtures on push/PR | 14, named individually in `ci.yml`'s `kernel-gate` |
| Of those, never run with fixtures anywhere in CI | 2 — `compression_oracle` (3), `write_oracle` (3). See T2. |
| Tests that pass without executing an assertion in CI | ~50 across six suites (T2), plus 1 that cannot fail anywhere (T1) |
| Static analysis | not re-run; CI enforces `clippy -D warnings` and `rustfmt`, and the prior review recorded both clean |
| Coverage | not measured this session |

The 171/171 figure is honest for what it measures and is not a statement about the crate:
`cargo test --lib` is the unit tier, and the crate's own documentation is emphatic that the
unit tier is necessary-but-not-sufficient. The number that moves with correctness here is
how many oracle suites ran against fixtures the kernel wrote, and T2 is about that number
being 14 rather than 20 on the path most changes take.

---

## What to fix first

Ordered by value per unit of risk.

0. **T2, then T1.** Ahead of everything else, because everything else is judged by the
   tests. Add the six missing suite names to `kernel-gate` — one edit,
   `.github/workflows/ci.yml`, and 50 tests start being able to fail on a pull request.
   Then fix T1 so the LZO padding branch has a unit test that calls the decoder, since
   two of those six suites need fixtures the CI scripts cannot build yet and `compression_oracle`
   is one of them. Do these before any refactor below: they are what will tell you the
   refactor was safe.

1. **H1, H2, M14, L19, M15** — the stale claims. Five doc edits, zero code change, zero
   test risk, and they are the findings most likely to mislead someone who is not you.
   H1 first: it is the load-bearing one, and it is a deletion rather than a rewrite.

2. **H5** — the split refusal, plus the two tests pinning it. Small, but it is a *false*
   statement in a message designed to tell a caller what to do, and it cannot be fixed
   without touching `src/leaf_edit.rs:286` and `tests/leaf_edit_oracle.rs:254`. Change
   those to assert the condition rather than the sentence while you are there.

3. **H6** — turn the silent `if info.data.len() >= 4` into a refusal, and route it through
   `free_space_info::EXTENT_COUNT`. This is the only finding on the list that can produce
   a wrong filesystem, and it is three lines.

4. **H4** — make `merge_adjacent` `pub(crate)` and call it. Two lines, deletes seven, and
   brings a unit-tested implementation to the untested copy.

5. **H3** — change `BlockVisitor` to carry `&TreeBlock` and delete the three hand-rolled
   item loops. The largest change on the list and the one with the best return: it removes
   four bare `25`s, three silent skips, and the crate's only production code that decodes
   items without the decoder. Do it after H4 so the diff stays legible.

6. **M7** — centralise the constants, starting with the three `root_item` modules. Merge
   into `fs.rs`'s and carry `transaction.rs`'s "AT 160, not 16" note across; that note is
   the only reason this is above a mechanical tidy-up. Then `ROOT_ITEM_KEY` into
   `chunk::key_type`, which is where every other key type already lives.

7. **M10** — decide about `insert_or_split`. Not a refactor; a question. Wire it up or
   remove it, but do not leave an untested split path in a crate that measures everything
   else.

8. **M8, M9, M12, M13, M16** — the structural work. All worth doing, none urgent, and M13
   in particular is worth doing once and properly rather than quickly.

9. **T3, T4, T5, T6, T7** — the test tightening. T4 and T6 are one `assert!` each replacing
   an `eprintln!`, and both are backstops the suites around them already have. T5 is the
   larger one and the one to do while the fixtures are fresh in mind: the pattern to adopt
   is the one `extent_write_oracle` and `free_space_oracle` already use — skip loudly, then
   assert at the end that something was checked.

L17, L18 and L20 are five-minute changes to fold into whatever branch touches those files.

The 2026-08-25 review's H1 (`checksum.rs`) is still the best single move for the *reader*,
and it is independent of everything above.
