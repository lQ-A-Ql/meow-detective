# Changelog

Notable changes to `rust-fs-btrfs` (published as `am-fs-btrfs` until its last version), newest first. This is a `0.x` crate, so the
**minor** is the compatibility boundary: a minor bump may break API, a patch
never does.

## [Unreleased]

## [0.10.2] — 2026-10-07

A tooling release: the library and its C ABI are unchanged.

### Fixed

- **The tools are released again, this time for certain.** 0.10.1 was
  published to crates.io by CI, but its GitHub release still has no tarballs:
  rust-fs-core 0.3.6's shared release workflow ran `scripts/core.sh
  package-cli`, a shim no repository carries since the family's scripts run in
  place, and both packaging legs failed with `No such file or directory`.
  rust-fs-core 0.3.7 runs `package-cli.sh` in place from the sibling it clones.

### Changed

- **rust-fs-core 0.3.7.** Cargo, both lockfiles and every workflow that clones
  the rust-fs-core sibling move to it.

## [0.10.1] — 2026-10-06

A tooling release: the library and its C ABI are unchanged.

### Fixed

- **The tools are released again.** 0.10.0's GitHub release has no tarballs:
  its crate was published by hand, CI's repackaged `.crate` did not match the
  published one, and the release stopped at the attestation check. 0.10.1 is
  published by CI through crates.io trusted publishing, and its release carries
  the darwin-arm64 and linux-x86_64 tarballs with their attestations.

### Changed

- **The family's scripts run in place from rust-fs-core 0.3.6**, and this
  repository keeps no copy of them (rust-fs-core#212).
- **A release's notes are its CHANGELOG section**, written by github-guard's
  changelog extraction through agent-skills' `changelog-notes` action, and a
  tag the CHANGELOG does not describe stops before anything is published.

## [0.10.0] — 2026-10-06

### Changed

- **Published as `rust-fs-btrfs`, the repository's name.** The crate was `am-fs-btrfs`
  until its last version, which stays on crates.io pointing here. A
  dependent changes one line in `Cargo.toml`; the import (`fs_btrfs`) and the C symbols are unchanged.
- **Depends on `rust-fs-core` 0.3.0**, the same library under its new name.

## [0.9.0] — 2026-10-06

**Breaking:** `Error` gains the variant `InvalidGeometry`, so this release is
0.9.0. A `match` over `Error` without a wildcard arm needs one more arm.

### Renamed

- **The last version published as `am-fs-btrfs`.** The crate is renamed to
  `rust-fs-btrfs`, the repository's name; every later version is published under
  that name only, starting at 0.10.0. The description and the README say where
  the crate went. The import is unchanged: `use fs_btrfs::...` keeps working.

### Added

- **`fsck.btrfs`, and `fs_btrfs::check` under it (#260).** Checks a volume
  without changing it, as `btrfs check --readonly` does: tree-block checksums
  and keys, extents against the trees that reference them, block groups,
  chunks against device extents, the free-space tree, and the namespace. Exit 0
  clean, 4 problems found, 8 could not check, 16 usage (a repair request is
  refused). Agrees with `btrfs check` on twelve clean fixtures and eight kinds
  of damage.
- **`mkfs.btrfs`, and `fs_btrfs::mkfs` under it (#259).** A single-device
  filesystem with the standard formatter's defaults, laid out as it lays one
  out: three chunks and the nine trees a fresh filesystem has, every item in
  its encoding, measured from its output. `btrfs check --readonly` and the
  kernel accept the result at node sizes from 16 to 32 KiB, with each of the
  four checksum types, from 128 MiB to 60 GiB (`tests/cli_mkfs_kernel.rs`).
- **`Error::InvalidGeometry`**, for a format that cannot be made: a device too
  small, a node size out of range, a label too long. `EINVAL` through the C
  ABI.

## [0.8.1] — 2026-10-03

### Added

- **A release attaches the command-line tools.** A `v*` tag now builds
  `am-fs-btrfs-<version>-darwin-arm64.tar.gz` and `…-linux-x86_64.tar.gz`
  with `scripts/package-cli.sh`, attests their build provenance and
  attaches them to the GitHub release beside the `.crate`; v0.8.0 shipped
  the crate alone. `tests/release_attestation.rs` fails a workflow that
  stops doing so.
- **A copy-on-write file can be written (#61, first slice).**
  `Filesystem::write(&mut self, ino, offset, data)` writes a regular file
  and commits the result. A `nodatacow` file is written in place, as
  `write_at` does; any other file has each extent the range touches copied
  into a newly allocated data extent with the write applied, the file's
  `EXTENT_DATA` items repointed, the extent tree, free-space tree, block
  group `used` and `bytes_used` updated, the inode stamped, and the whole
  committed as one transaction, after which the mount is reopened on the
  new generation. Verified by `btrfs check` and a kernel readback in
  `tests/cow_write_oracle.rs`. Still refused, by name: checksummed files
  (the checksum tree is not written yet), writes that grow a file or fill
  a hole, inline, preallocated and compressed extents, and extents a
  snapshot or another file shares. The C ABI and the command-line tools
  still call `write_at`.

### Changed

- **The command-line plumbing comes from am-fs-core's `cli` feature.**
  The hand-kept copy in `src/cli/common/` is deleted and `fs.btrfs` uses
  `fs_core::cli` (antimatter-studios/rust-fs-core#177), as every other
  driver's tools now do. This crate's `cli` feature turns on
  `am-fs-core/cli`, so the static library still gains nothing, and
  `clap_complete` and `clap_mangen` are no longer direct dependencies.
  The tools' names, output, man pages and completions are unchanged.

## [0.8.0] — 2026-09-30

### Breaking

- **In-image paths cross the C ABI as bytes, not UTF-8 (#214).**
  `fs_btrfs_stat`, `_dir_open`, `_read_file`, `_readlink`, `_listxattr`,
  `_getxattr`, `_can_write_in_place` and `_write_file` read their
  `const char *` as the bytes up to the NUL and compare them byte for byte
  against the names in the image, across subvolume boundaries too; they no
  longer decode it. `fs_btrfs_dir_next` already reported names as raw
  bytes, so the library handed out names it then refused with "not valid
  UTF-8". A path naming no file is now reported as missing (`ENOENT`), not
  as a bad argument.

  **Source-compatible for every caller passing UTF-8**, because UTF-8 is a
  byte string too. `lookup_path_bytes`, `resolve_path_bytes` and
  `list_path_bytes` carry the resolution and the `&str` forms wrap them, so
  the Rust API is unchanged. `fs_btrfs_mount` and `fs_btrfs_mount_rw` keep
  their UTF-8 decode: their argument is a path on the host filesystem, not
  an in-image name.

### Added

- Releases carry a build-provenance attestation: the published `.crate` is
  attached to the GitHub release for its tag, checked first against the
  crates.io checksum, and verifiable with `gh attestation verify` (see the
  README, "Verifying a release").
- **`rust-fs-btrfs`, the command-line tools, as one multi-call binary**
  (#225). Behind the `cli` feature, so the static library gains no
  dependency. Installed as `rust-fs-btrfs` with `fs.btrfs` linked to it;
  `--version` names the crate, `rust-fs-btrfs doctor` says whether each
  name on PATH is this program and how to fix one that is not, and every
  failure is a structured JSON error whose code is the exit status.
  `mkdir`, `set label` and `resize` answer `not implemented` (exit 3).
  `chore cli:install` stages the tools and `chore test:cli` tests them as
  installed; CI runs both on every pull request, whatever its base.
- **`fs.btrfs ls`, `read`, `get` and `info`** (#225). `ls` lists a
  directory as typed JSON entries and marks subvolumes; a path into a
  subvolume or a snapshot is followed. `read` streams a file's bytes to
  stdout or `-o FILE`. `get`/`info` report the shared envelope with the
  Btrfs specifics under `btrfs`, from the superblock alone. `--offset`
  reaches a volume inside a whole-disk image. Tested against a volume the
  kernel populated (`test-disks/cli/`), with `get` held to `btrfs
  inspect-internal dump-super`.
- **`fs.btrfs write`, for the one write this library can make** (#225):
  an existing NODATACOW file overwritten in place with exactly as many
  bytes from stdin as it holds. Every other write is refused with exit
  status 3 and the library's reason, and creating a file, like `mkdir`,
  is blocked on #61. After a write, `btrfs check --readonly` finds the
  volume clean and the kernel reads back the bytes written.
- **Man pages and shell completions, and the release tarball's layout**
  (#225). `rust-fs-btrfs generate man|completions SHARE` writes a section 1
  page per name and per `fs.btrfs` subcommand, and zsh, bash and fish
  completions, from the clap commands the tools parse with (clap_complete
  and clap_mangen, MIT/Apache-2.0, behind `cli`). `scripts/package-cli.sh`
  stages `am-fs-btrfs-<version>-<platform>.tar.gz` as an install prefix and
  refuses it unless the layout, the links, the documentation and every
  name's `--version` are right; the `cli` CI job builds it on every pull
  request.

### Fixed

- `fs_btrfs_last_errno()` and `fs_btrfs_last_error()` describe the calling
  thread's most recent call rather than its most recent failure (#232).
  Every entry point except `fs_btrfs_umount` and `fs_btrfs_dir_close`
  clears them on entry, so a clean end of directory after any earlier
  failure on the thread reports errno 0, as the header always promised.
- **A transaction keeps each block group's `used` count true** (#223).
  A plan releases every block it moves at its old address and allocates
  it at a new one, and the two are often in different block groups.
  `bytes_used` still added up, but no `BLOCK_GROUP_ITEM` was rewritten, so
  the group a block moved into was under-counted and the group it left
  over-counted, which `btrfs check` reports as "block group [...] used X
  but extent items used Y". The plan now includes the extent-tree leaf
  holding each group's item whose count moves, and `render_plan` moves
  `used` by what it allocates in the group less what it releases there.

- **A commit writes `ROOT_ITEM.generation_v2` with `generation`** (#195).
  `render_plan` updated a moved root's `generation` and left its second
  copy stale, so the kernel mounting the result logged "mismatching
  generation and generation_v2" for every moved root and reset the root's
  extended fields (subvolume UUIDs and times). Found by the new
  crash-consistency test, the first to read the kernel's log after
  mounting a commit of this crate.
- **A transaction touching a block group whose free-space records span two
  leaves is refused, instead of recording that free space twice.** The
  free-space tree rewrite wrote every run of a group into the leaf holding
  its `FREE_SPACE_INFO`, and the next leaf kept its own copies of the
  group's tail: `btrfs check` then found "free space extent ... overlaps
  with previous". Where the group did not fit the first leaf the render was
  refused by a message about leaf sizes. `render_plan` now refuses any plan
  touching such a group, by name, before it renders a block. A touched
  group that no block group item describes is refused as well, rather than
  carried through with its old records (#177).

## [0.7.0] — 2026-09-27

### Breaking

- **`fs_btrfs_readlink` follows the family readlink contract** (#205),
  shared by every driver so a layer above needs one shape for the call:
  success returns the target length excluding the NUL, as `readlink(2)`
  does, with the target and a terminator written; a buffer smaller than
  the length plus one is `-1`/`ERANGE` with a message naming the size
  needed and nothing written; a NULL `fs`, `path` or `buf` is
  `-1`/`EINVAL`; and any other failure sets this call's errno. What
  changed here is the errno: a NULL argument was `EIO` (or `ENOENT` for
  the path) and is `EINVAL`; a zero `bufsize` was `EIO` and is `ERANGE`;
  and a path that is not a symlink was `EISDIR`, which is wrong for a
  regular file, and is `EINVAL` as `readlink(2)` reports it. A caller
  testing the result `== 0` for success must test `>= 0`.

### Added

- **The parsers are fuzzed, on two tiers.** Nothing here had a fuzz
  target. `fuzz/` holds six `cargo-fuzz` targets — superblock, tree
  block, inode item, directory items, xattr items and chunk item — and
  runs nightly on a bounded budget; `tests/fuzz_decoders.rs` is the gate,
  47,616 deterministic cases in under a second on the stable toolchain.

  The tree-block target **re-stamps the checksum after mutating**, and
  that is what makes it test btrfs rather than test crc32c:
  `TreeBlock::parse` verifies the checksum before it looks at anything
  else, so without a re-stamp every mutated block is rejected on the
  first line and the item walk is never reached. It also passes the
  block's own recorded `bytenr`, because a block that does not claim to
  live where it was read from is refused by a second identity check
  behind the first. A crafted image satisfies both — whoever wrote it
  chose those numbers — so satisfying them is what makes the test
  resemble the threat. `a_restamped_tree_block_still_parses` exists to
  prove that path is actually reached; without it, a `restamp` computing
  the wrong digest would look exactly like a target finding no bugs.

  There is deliberately no whole-image target: the smallest filesystem
  `mkfs.btrfs` will make is 16 MiB even with `--mixed`, over the 10 MiB
  ceiling github-guard enforces, and the mount path is already the
  best-covered thing here (#65).


- **A path can cross into subvolumes.** `Filesystem::resolve_path` walks a
  path through any number of subvolume boundaries and returns the inode
  together with the read-only handle of the tree it belongs to
  (`PathTarget`). `read_path` and `list_path` use it, so `/sub/inner/c.txt`
  reads. A crossing follows the kernel's rules: the entry must be backed by
  a `ROOT_REF` from the tree it sits in. When one is missing, which is how
  a snapshot's copy of a nested subvolume's entry looks, the result is the
  empty directory the kernel shows there (inode 2), not the live
  subvolume. `lookup` and `lookup_path` still stop at a boundary, and their
  refusal now points at `resolve_path` (#62).
- **Extended attributes can be read.** `XATTR_ITEM` was parsed as far as
  its shape and the value thrown away, so an attribute sitting in the
  tree could not be reached from anywhere. `Filesystem::list_xattrs` and
  `Filesystem::get_xattr` return names and values, and the C ABI gains
  `fs_btrfs_listxattr` and `fs_btrfs_getxattr` with the same signatures
  and semantics `fs_ext4_*` already has.
  - Names that hash to the same key share one item, and every record in
    it is returned. `tests/xattr_oracle.rs` sets two such names — found
    by searching the hash — and requires both to come back.
  - Btrfs stores the namespace prefix as part of the name, so nothing is
    expanded on the way out: `trusted.x` comes back spelled in full.
  - A zero-length value is distinct from an absent attribute, in both
    APIs.
- A fixture carrying attributes, built by
  `scripts/build-xattr-fixtures.sh` from a mounted filesystem, with
  `getfattr`'s own dump recorded beside it as the reference answer.

### Changed

- **Mounting no longer reads the whole filesystem tree.** The fs tree was
  loaded into memory at mount -- 2937 device reads and 12 MB for 20,000
  files before the first question. Items are now found by descending the
  tree when asked, with the verified blocks a descent reads kept in a
  node cache of `NODE_CACHE_BLOCKS` (1024) that a commit clears
  (`btree::Tree::with_cache`). The same volume mounts
  in 5 reads; a later path resolution through directories already listed
  makes none.
- **A lookup fetches the name by key.** `Filesystem::lookup` listed the whole
  directory and scanned it for the name, so each path component cost the
  size of its directory: 20,000 lookups in a 20,000-entry directory took
  96 s. It now reads the `DIR_ITEM` filed under the name's hash and matches
  within it (names that share a hash are packed there), 20 ms for the same
  work. `.` and `..` still resolve to nothing, as before.
- `dir::parse_dir_items` and the new `xattr::parse_xattr_items` share one
  record walk, so the bounds arithmetic over a packed
  `struct btrfs_dir_item` sequence exists once rather than twice.

### Fixed

- **A read cannot follow an extent item's window outside its extent.** The
  window was bounded by `ram_bytes`, a field of the same item, so an item
  that raised both passed and the read answered with whatever occupied the
  addresses past the extent — another file's data, or a tree block, with no
  error. The write path has checked against the extent tree's own record
  since #156; the read path does now too. `extent_item` seeks to its key
  instead of walking the whole extent tree, because a read does this per
  extent where a write did it once (#188).
- **A compressed stream that decodes short is refused anywhere but the
  file's last extent.** `decompress` padded a short decode with zeros up to
  `ram_bytes`, which is right for the last extent — the tail of the final
  sector holds nothing — and wrong everywhere else, where it turned a
  truncated or damaged stream into a run of zeros a caller cannot tell from
  data. The caller now says which case it has through
  `compression::ShortDecode`: the read path pads only the extent that
  reaches the end of the file, and an inline extent, which is the whole of a
  small file (#189).
- **A new tree block is never placed on a superblock copy.** The extent
  tree doesn't record the superblock copies, and the allocator took its
  gaps as free. On a 256 MiB volume it handed out the block whose DUP copy
  sits at 64 MiB, where a commit then writes the second superblock.
  `plan_transaction` and `find_metadata_block` now skip the `stripe_len`
  row holding each copy, as the kernel does (#175).
- **A commit fills its backup-root slot.** The superblock keeps the roots of
  the last four commits for `btrfs rescue` and `usebackuproot`, and a commit
  by this driver left its slot describing an older commit the kernel made.
  `Filesystem::commit` now reads the extent, filesystem, device and checksum
  roots out of the root tree it writes and fills the slot for its
  generation (`super_write::write_backup`), and refuses, before writing
  anything, a root tree that does not parse.
- **An in-place write no longer overwrites data a snapshot still reads.**
  `write_at` took an extent item's single reference to mean one reader. A
  snapshot of a subvolume whose tree is a node leaves data extents at
  `refs 1`, so a `chattr +C` file's write changed the snapshot's copy as
  well. The write and `can_write_in_place` now also refuse an extent from
  a generation at or before the fs tree's `last_snapshot`, as the kernel's
  nocow check does (#173).

## [0.6.2] — 2026-09-06

### Fixed

- A tree walk visits each block once. Without a visited set, a chunk
  tree that points back into itself made the mount loop.
- Extent lengths are bounded before they become allocation sizes, and a
  chunk cannot send a write outside the device it maps.

## [0.6.1] — 2026-09-04

### Changed

- **"Find a tree's root" means one thing.** Three lookups implemented it
  differently — disagreeing on the search bound and on the match order — so
  which one you called decided what you got. The visitors are now handed an
  already-parsed block rather than each re-parsing it.
- **One definition of the endian readers**, instead of a set per module.
- Refusals name the way out rather than ending in a dead end. An error that
  says only "unsupported" leaves the caller with nothing to do next.

## [0.6.0] — 2026-08-29

### Added

- **A whole transaction is written and the kernel judges it** — the plan is
  closed over its own bookkeeping, turned into the blocks it says to write, and
  then handed to `btrfs check`.
- **The free-space tree is kept in step**, and the result passes `btrfs check`.
  Settled first why the tree names more block groups than exist, which was a
  property of how it lays its items out rather than a bug.
- **Pool reads: every device is opened and each mapping is answered from the
  right one.** Previously a pool member could be read as if it were the whole
  filesystem.

### Fixed

- **One device of a multi-device pool is refused rather than read as the whole
  thing.** Reading it standalone returns whatever that disk happens to hold and
  invents the rest.
- Stripe floors that the toolchain bump revealed.
- A docblock that described behaviour the code did not have, seven test suites
  that were never being run, and three copies of one offset.

### Changed

- Pinned toolchain moves to 1.95.0, in lockstep with the rest of the family.

## [0.5.0] — 2026-08-26

### Added

- **The write path, exposed through the C ABI**: `nodatacow` files can be
  overwritten in place. CoW writes are not covered — see the README.

## [0.4.0] — 2026-08-25

### Added

- **Compressed extents are decoded** — zlib, LZO and zstd.

## [0.3.0] — 2026-08-25

### Added

- Initial public release: superblock parser, chunk address map, B-tree node and
  leaf reads with tree walking, inode and directory items, and the filesystem
  handle (mount, resolve, list, read).
- `fs_core` mounting and a C ABI aligned with the sibling drivers.
- Compression flags are accepted at mount, with the refusal paths covered.
- **Cross-validation against real media**, including multi-level tree descent,
  which is what caught the `metadata_uuid` handling. The oracle comparison runs
  before mounting, and mounts operate on copies.

### Fixed

- `readlink` refuses a buffer too small for the target instead of truncating
  the path silently.
- Both `metadata_uuid` renderings the reference tooling emits are tolerated.

[0.10.2]: https://github.com/antimatter-studios/rust-fs-btrfs/compare/v0.10.1...v0.10.2
[0.10.1]: https://github.com/antimatter-studios/rust-fs-btrfs/compare/v0.10.0...v0.10.1
[0.10.0]: https://github.com/antimatter-studios/rust-fs-btrfs/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/antimatter-studios/rust-fs-btrfs/compare/v0.8.1...v0.9.0
[0.8.1]: https://github.com/antimatter-studios/rust-fs-btrfs/compare/v0.8.0...v0.8.1
[0.8.0]: https://github.com/antimatter-studios/rust-fs-btrfs/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/antimatter-studios/rust-fs-btrfs/compare/v0.6.2...v0.7.0
[0.6.2]: https://github.com/antimatter-studios/rust-fs-btrfs/compare/v0.6.1...v0.6.2
[0.6.1]: https://github.com/antimatter-studios/rust-fs-btrfs/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/antimatter-studios/rust-fs-btrfs/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/antimatter-studios/rust-fs-btrfs/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/antimatter-studios/rust-fs-btrfs/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/antimatter-studios/rust-fs-btrfs/releases/tag/v0.3.0
