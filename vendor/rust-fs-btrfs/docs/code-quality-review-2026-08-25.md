# Code quality review — 2026-08-25

**Scope:** `src/`, 4,989 production lines across 10 files (test modules excluded from
every count below).
**Findings:** 1 high, 3 medium, 1 low. No fixes applied — this is a read of the code
as it stands.

This crate is in good condition — no duplication, no unnamed offsets, and a module
layout that follows the format's own structure. The findings are about a few long
functions, one file that has outgrown its name, and the deepest nesting in the crate
sitting in the code least able to afford it.

---

## H1 — `superblock.rs` is 946 lines, and only part of it is the superblock

**`src/superblock.rs`**

The largest file in the crate. `Superblock::parse` (108 lines) and `validate` are
here as expected, but so are the four checksum implementations the format allows —
CRC32C, XXH64, SHA-256 and BLAKE2b — and the dispatch between them.

Checksum selection is a volume-wide property read from the superblock, so the
association is not arbitrary. But it is a separate concern with separate dependencies
(four crates, none of which the superblock parsing needs), and a reader looking for
"how do we verify a tree block" would not think to look here.

**Shape of the fix.** A `checksum.rs` holding the four algorithms and the dispatch,
leaving `superblock.rs` to parse and validate the superblock. `rust-fs-xfs` and
`rust-fs-ext4` both already have exactly this file.

---

## M2 — 36 lines indented 24 columns or deeper, mostly in `chunk.rs` and `btree.rs`

**`src/chunk.rs`, `src/btree.rs`**

Six levels of nesting and beyond, and the placement is what makes it worth raising:
`chunk.rs` maps logical addresses to physical ones. If that mapping is wrong the driver
does not fail — it reads the wrong bytes from the right-looking place, which the file's
own header comment says is the danger it is written to avoid.

That is precisely the code where a reader most needs to see the control flow at a
glance, and the nesting is where it stops being possible. `map_mirror` (68 lines) is
the densest of it, with a `match` on profile inside a stripe loop inside a bounds
check.

Early returns for the refusal cases — RAID5/6 and unsupported profiles are already
rejected, just not early — would flatten most of this.

---

## M3 — Nine functions of 60 lines or more

**`src/superblock.rs:605 parse` (108), `src/fs.rs:356 decode_extent` (83),
`src/chunk.rs:430 validate` (80), `src/btree.rs:431 parse` (72),
`src/fs.rs:164 mount` (71), `src/chunk.rs:557 map_mirror` (68)**

The `parse` functions are the acceptable case: a flat field-by-field mapping is at one
level of abstraction throughout, and splitting it makes it harder to check against the
format documentation rather than easier.

Two are worth revisiting:

- **`fs.rs:mount` (71 lines)** implements the four-step bootstrap — superblock, system
  chunk array, chunk tree, root tree — which is the single most load-bearing sequence
  in the crate. Each step has a name already, in comments.
- **`chunk.rs:validate` (80 lines)** checks several unrelated properties, so any of a
  dozen conditions produces one generic error. `superblock.rs` has already been split
  this way (`validate_geometry`, `validate_tree_roots`, `validate_sys_chunk_array`,
  `validate_device_identity`) and the same treatment applies here.

---

## M4 — `capi.rs` is 704 lines with a five-parameter entry point

**`src/capi.rs`**

Large but flat — a list of ABI entry points with a consistent shape, which is the one
kind of long file that stays navigable. `fs_btrfs_read_file` takes five parameters
because the ABI says so, and should be left alone.

Noted for completeness rather than for action.

---

## L5 — One `#[allow(dead_code)]` with no stated reason

**`src/fs.rs`, the `file_extent` offsets module**

The suppression is correct and the module even explains the underlying decision:

> The full field list is kept even where a read-only driver does not consult every
> one, because the offsets that follow are only checkable against the format
> documentation when the fields between them are named too.

That reasoning sits above the module; the `#[allow]` sits below it. Moving them
adjacent, or referencing one from the other, would stop a later reader treating the
suppression as unexplained. A one-line change.

---

## What is good

- **No duplication at all.** Zero repeated eight-line blocks across 4,989 lines.
- **No unnamed multi-digit offsets.** Every structure has a documented `offsets`
  module, including fields nothing currently reads — which is the harder half of the
  discipline and the half that makes the rest checkable.
- **The error type distinguishes kinds of refusal.** `UnsupportedProfile`,
  `UnsupportedChecksum` and `UnsupportedFeature` are separate variants so a caller can
  tell "this volume uses RAID6" from "this volume uses a feature we lack", which the
  C ABI then maps deliberately rather than collapsing.
- **`chunk.rs` states its own hazard in its header** — that a wrong mapping returns
  plausible bytes rather than failing — and the code is written to refuse rather than
  guess.
- **Cross-validation against `dump-super`, `dump-tree` and filesystems the kernel
  wrote**, with fixtures that assert they still contain the features they cover.
- **`clippy -D warnings` and `rustfmt` are clean**, and CI enforces both.

## Suggested order

H1 first — it is a file move with no logic change, and it puts the checksum code where
the sibling crates keep theirs. Then M3's `validate`, following the pattern already
established in `superblock.rs`. M2 last, since flattening `chunk.rs` is easier once
`validate` is out of it.

Nothing here is urgent.
