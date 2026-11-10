# Human-code findings — status

Tracks every **High** and **Medium** finding from
[`human-code-report-2026-08-28.md`](human-code-report-2026-08-28.md). The report
predates the work; this is the current position. Updated 2026-08-31.

**27 findings** — 6 High, 10 Medium, 5 Low, plus 7 tests-that-cannot-fail. This
covers the 16 High and Medium.

| | High | Medium |
|---|---|---|
| Fixed | 6 | 6 |
| Left for a human decision | 0 | 4 |
| Fixable, not yet done | 0 | 0 |

The four Medium items left for a human decision are M8/M9 (two transaction-path
god functions), M12 (a rename whose right answer depends on what the caller
should understand the call to cost), and the `items_of` half of M16.

---

## High

### H1 — `render_plan`'s doc block stated the opposite of what it does — **fixed earlier**

[#46](https://github.com/antimatter-studios/rust-fs-btrfs/pull/46).

### H2 — `fs.rs` said decompression is not implemented, in the module that decompresses — **fixed**

The module's own list of "things we refuse rather than guess at" opened with
*"Compressed extents. Decompression is not implemented."* All three btrfs codecs
— zlib, LZO and zstd — are decoded, in `compression.rs`, and `fs.rs` calls it at
two sites.

True when written; not for some time. The list no longer claims it, and the
module says so explicitly so the next reader does not have to check.

### H6 — a short `FREE_SPACE_INFO` item silently kept a stale extent count — **fixed**

```rust
let mut info = item.clone();
if info.data.len() >= 4 {
    info.data[0..4].copy_from_slice(&(runs.len() as u32).to_le_bytes());
}
```

The `if` wrote the count when the item was long enough and **said nothing when
it was not** — so a short item kept a stale count while its runs were rewritten
underneath it, and the free-space tree disagreed with itself with no signal that
anything had happened. Nothing could test the branch, because it produced no
observable effect.

Refused now. The offsets also come from `block_group::free_space_info`, which is
where the *reader* already gets them; this side was spelling them out by hand,
so the two could drift.

### H5 — a refusal that told callers to give up on something the crate does — **fixed**

`insert` refused an over-large item with:

> Splitting a leaf is not implemented — where the kernel puts the boundary is a
> policy this has not measured.

Both halves false. `leaf_edit::split` is **fifty lines below**, with
`tests/split_oracle.rs` checking it against leaves the kernel really split, and
`insert_or_split` does exactly what the message says cannot be done. The module
doc directly above prints three *measured* splits, and `docs/cow-transaction.md`
has a section on the measurement and on why the boundary is deliberately not
copied.

The house style is what made this expensive: the message is long and specific
precisely so a caller knows what to do next, and what it told them to do was give
up. It now names `insert_or_split` and says why `insert` itself refuses — a caller
who meant to write exactly one block should find out rather than get two.

`tree_write::build_leaf` had the milder version of the same disagreement inside one
function: its doc says splitting is "the caller's decision, not this function's"
(correct) while its error said it "is not implemented". That one now names both
`leaf_edit::split` and `insert_or_split`.

**Three tests pinned the false sentence; all three now assert the condition
instead.** That is the part worth keeping. `src/leaf_edit.rs:286`,
`tests/leaf_edit_oracle.rs:254` and `tests/tree_write_oracle.rs:458` all matched on
the exact wording, so the claim had three tests holding it in place. They check
that the refusal names what went wrong and names the function that handles it —
properties a correct message keeps and a rewording cannot break.

The third was found by CI rather than by grep: it only runs under the kernel-
validation job, so a local `cargo test` never reached it. Fitting, in that the
last test still asserting splitting was unimplemented was the one hardest to run.

### H3 — the write path hand-rolls leaf parsing `btree.rs` already does — **fixed**

The root cause was one line: `for_each_tree_block` had a fully parsed `TreeBlock`
in hand and passed the visitor `block.bytes()`, so all three visitors re-parsed
the leaf themselves — `25` as a bare literal in four places, the key at `+0..17`,
the data offset at `+17..21`.

`BlockVisitor` now carries `&TreeBlock`. All three hand-rolled loops are gone,
along with the `HEADER_SIZE` / `header_offsets` imports they needed. The bounds
decision the two loops made inline — `if off + BYTENR + 8 <= block.len()` — is
now `TreeBlock::item_data`'s `Option`, in `root_item_bytenr`, where a reader can
see it being made.

### H4 — the free-run merge in `apply_free_space` duplicates `merge_adjacent` — **fixed earlier**

This entry was stale. `block_group::merge_adjacent` is `pub(crate)` and
`transaction.rs:838` calls it; there is no second copy.

---

## Medium

### M7 — six declarations of `ROOT_ITEM_KEY`, four of `root_item::BYTENR` — **fixed earlier**

[#46](https://github.com/antimatter-studios/rust-fs-btrfs/pull/46). One
declaration now.

### M10 — `leaf_edit::insert_or_split` had no callers and no tests — **fixed earlier**

### M14 — `lib.rs`'s status section described an earlier crate — **fixed earlier**

### M8, M9 — `apply_free_space` at 107 lines and four jobs; `render_plan` at 106 across four abstraction levels — **needs your decision**

Both accurate. Both are transaction-path functions where the ordering *is* the
correctness argument, and splitting them without a test on the seams trades a
readability gain for a risk in the part of the crate that writes to disk.

### M12 — `mine` reads as a set-membership test and is a full B-tree traversal — **needs your decision**

A rename is the fix and the right name depends on what the caller should
understand it to cost.

### M11 — four implementations of "find a tree's root" — **fixed, and the three plain copies did not agree**

One `fs::root_item_target(tree, root_tree, objectid)`. The copies differed in two
ways, neither of which any test could distinguish because a real `ROOT_ITEM` is
439 bytes and neither disagreement can trigger on one:

- two required `data.len() > root_item::LEVEL` (238) and one required
  `data.len() > root_item::BYTENR + 8` (184), so a truncated item of 185..=238
  bytes was **accepted by one and rejected by the others**;
- two stopped at the first match and one kept scanning and took the **last**, so
  a root tree with two items for the same objectid would have read differently
  depending on which caller asked.

Both are settled, with the reasoning in the doc: the bound is the bytes actually
read (a bound of `LEVEL` refuses items the function could answer from), and the
first match wins (a tree with two is already malformed, and reading on does not
make the answer better). Five tests pin exactly those decisions — including the
minimal-length item that two of the three old copies would have refused.

`transaction::root_item_leaf` stays separate, because it answers a different
question (*which leaf holds* the item, not what it points at), but it is now
three lines because H3 gave it `block.body.items()`.

### M13 — the read-closure + `Tree` boilerplate eleven times — **fixed**

`Filesystem::pool_reader()` returns a `PoolReader` that owns the closure;
`reader.tree()` borrows it. Ten of the eleven sites are now two lines.

The borrow checker is why this had not been factored, and the fix is worth
recording: the closure borrows three fields and the `Tree` borrows the closure,
so one function cannot return both. Returning the *reader* and taking the tree
from it splits the two borrows across two statements, which is all that was
needed.

### M15 — the example invalidates the free-space tree for a reason that no longer holds — **fixed (the reason, not the flag)**

The flag stays `true`, and the comment now gives the reason that is actually
true. The old one — "this transaction does not maintain the free-space tree" —
stopped being true when `apply_free_space` landed: `render_plan` rewrites the
free-space tree's extents alongside the extent tree's, and *refuses* a block
group recorded as a bitmap rather than skipping it.

The reason now is that the example does not verify what it wrote. Clearing the
validity bit is the format's own way of saying "believe the extent tree, not this
cache", which is the honest setting for a file whose point is the transaction
shape. Setting it to `false` here — in an example nothing checks — would assert a
property this file does not test; the oracle suite, which runs `btrfs check`
inside a VM, is where that assertion belongs.

### M16 — `le32`/`le64`/`items_of` copied across eight test binaries — **`le32`/`le64` fixed; `items_of` left**

`le32` and `le64` are in `tests/common/mod.rs` now, and eight binaries import
them instead of declaring their own.

They are still **hand-rolled there, not imported from `src/`**, and the doc says
why: these oracles decode leaves by hand on purpose. One that read a leaf through
`btree::TreeBlock` would be checking the writer against the reader instead of
against the disk, and the two agreeing is the one thing an oracle must not
assume. The decoder moved sideways, into the module the tests already share —
not inward, into the crate under test.

`items_of` is left. Its four copies return four different types (`Vec<OwnedItem>`,
`Option<Vec<OwnedItem>>`, `Vec<(DiskKey, Vec<u8>)>`, `Vec<(DiskKey, Range<usize>)>`),
so a shared version is a design job about what an oracle should be handed, not a
copy-paste removal.

### The endian readers, declared three times — **fixed**

`le64` was defined in `superblock`, `block_group` **and** `fs` — byte-identical
bodies — with `le32` twice. 100 call sites across the crate depend on them.

Nothing was wrong with any copy, and that is the point: a helper this small is
not duplicated because somebody misunderstood it, but because declaring it again
is cheaper in the moment than importing it. The cost lands on a reader checking
that the third copy still says `from_le_bytes`.

They stay in `superblock` rather than moving to an `endian` module, because the
superblock is the first thing anything parses and every other module already
imports from it. The doc now says btrfs is little-endian throughout and that
there is deliberately no big-endian half — a big-endian read here would be a bug,
and a helper for it would make the bug spellable.

**Both duplicates were covered, which is what made removing them safe.** Flipping
each to `from_be_bytes` in turn: `block_group` 4 tests, `fs` 20. After, flipping
the single definition fails **72**.

Three tests assert the byte order against literal bytes rather than against
`from_le_bytes`. That is the property worth pinning: a byte-order slip is
invisible to a round trip through this crate, because reader and writer would
agree with each other while disagreeing with the kernel.

---

## Verification

173 unit tests pass, unchanged in number. `chore lint` clean. H6 is the only
behavioural change: a `FREE_SPACE_INFO` item too short to hold its own count is
now refused rather than silently kept.
