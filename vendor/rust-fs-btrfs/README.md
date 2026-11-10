# rust-fs-btrfs

Pure-Rust, clean-room [Btrfs](https://btrfs.readthedocs.io/) driver. A reader for
the Btrfs on-disk format built over the shared
[`rust-fs-core`](https://github.com/antimatter-studios/rust-fs-core) block-device
trait, exposing a stable C ABI (`fs_btrfs_*`) for FFI from C/C++, Swift or Go.

Published on crates.io as `rust-fs-btrfs`; the library name is `fs_btrfs`.

Btrfs is a copy-on-write filesystem: nothing is overwritten in place, every
structure is a B-tree, and the physical location of any byte is resolved through
the chunk tree rather than computed from a fixed formula. That makes it a
different shape of problem from ext4 or NTFS — a reader has to bootstrap the
chunk tree out of the superblock's embedded system chunk array before it can
address anything else at all.

- **Clean-room** — written from the published on-disk format, not translated
  from kernel or `btrfs-progs` source
- **Permissive** — MIT, with a permissive dependency tree (no GPL/LGPL anywhere)
- **Cross-platform** — the driver reads images on Linux, macOS and Windows; only
  the test oracle needs a Linux kernel

## Status

Under active development. The table is the honest state of what is implemented,
not a roadmap.

It was last wrong in the other direction: it described a reader with the write
path "out of scope" and B-tree traversal, inodes and compression "planned", all
of which had been in for some time. A status table that understates is not
harmless — someone deciding whether this is usable reads it and concludes it is
not.

| Area | Support |
|------|---------|
| Superblock (primary at 64 KiB) | done — 216 field comparisons against dump-super |
| Superblock mirrors (64 MiB, 256 GiB) | done — every copy is read and the valid one with the highest generation is used; a read-write mount refuses unless that is the primary (#90) |
| Checksum: **crc32c** | done |
| Checksum: **xxhash64**, **sha256**, **blake2b** | done — all four verified against real media |
| System chunk array → chunk tree bootstrap | done |
| Chunk tree / logical→physical mapping | done — bootstrap array, then the full tree folded in |
| B-tree node + leaf traversal | done — walk and keyed search, on multi-level trees |
| Root tree, fs tree, extent tree | done |
| Inodes, directory items, extent data | done |
| Directory listing, lookup, path resolution | done |
| Symlinks | done |
| Extended attributes: read | done — `list_xattrs` / `get_xattr`, checked name by name against `getfattr` |
| Extended attributes: write | not yet — setting one means inserting into a tree a transaction has to commit |
| Profiles: single, dup, raid0, raid1, raid10 | done |
| Profiles: raid5/6 | refused explicitly, not guessed |
| Mixed block groups (`mkfs.btrfs -M`) | reads; covered by the fixture matrix |
| Subvolumes and snapshots: listing | done — id, path, parent, snapshot and read-only flags, checked against `btrfs subvolume list` |
| Subvolumes and snapshots: reading inside one | done — `open_subvolume` gives a handle over that tree; read-only |
| A path that crosses into a subvolume | done in `resolve_path`, which hands back the subvolume and the inode in it; `lookup_path` stops at the boundary and says which subvolume to open, because an inode number means nothing without its tree |
| Compression (zlib / lzo / zstd extents) | done — all three, verified against files the kernel wrote |
| Write path: overwrite in place | done — `nodatacow` files only, no journal needed |
| Write path: anything copy-on-write | partial — `Filesystem::write` overwrites bytes inside a file's existing extents and commits the transaction (#61). It refuses a write that grows a file or lands in a hole, a checksummed file (most files: the checksum tree is not written yet), and inline, preallocated, compressed and shared extents (#261). The CLI and the C ABI do not use it yet (#274) |
| C ABI (`fs_btrfs_*`) | done, including the write entry points |

## Command-line tools

`fs.btrfs` reads a Btrfs image or device **directly**: no mount, no kernel
driver, no VM. It is an escape hatch for getting data off a disk that will not
mount, not a place to do real filesystem work.

```sh
chore cli:install                     # build and stage them in tmp/cli/bin
export PATH="$PWD/tmp/cli/bin:$PATH"   # the line cli:install prints
rust-fs-btrfs doctor                  # is every name on PATH this program?
```

One multi-call binary, `rust-fs-btrfs`, behind the `cli` cargo feature (the
static library gains nothing from it). Installed, `fs.btrfs` is a symlink to it;
`rust-fs-btrfs fs ...` is the same tool under the one name nothing else can
shadow, and `cargo run --features cli -- fs ...` works before anything is
installed. `--version` prints `<name> (rust-fs-btrfs) <version>`.

Metadata is JSON on stdout by default, `--text` for people. A failure is
`{"error": "...", "code": N}` on stderr and `N` is the exit status: 1 failed,
2 the command line was wrong, 3 the verb exists and this library cannot do it.

| verb | state |
|---|---|
| `ls [PATH]` | JSON entries: name, type, size, mode, mtime, inode, `subvolume`, and a symlink's target. A path into a subvolume or a snapshot is followed, as a mount shows it |
| `read PATH [-o FILE]` | the file's raw bytes: inline, compressed (zlib, LZO, zstd), sparse, inside a subvolume or through a snapshot |
| `get [KEY]` / `info [KEY]` | `fs`, `label`, `total_bytes`, `free_bytes` (from `bytes_used`), `block_size` (the sector size), `dirty` (a log to replay, or the error flag), and `btrfs.*`: fsid, metadata UUID, node size, checksum type, device count, generation, feature names. From the superblock alone, so a volume that will not mount still answers |
| `write PATH` | **only the in-place case**: an existing NODATACOW file (`chattr +C`), overwritten with exactly as many bytes from stdin as it holds. A new file, an ordinary copy-on-write file, a snapshotted, compressed or inline extent, a different length, or a path inside a subvolume is refused with exit 3 and the library's reason. An ordinary file waits on the CLI using the library's copy-on-write writes ([#274][i274]); creating, removing and resizing wait on [#262][i262] |
| `mkdir` | not implemented (exit 3): needs directory edits, [#262][i262] |
| `set label VALUE` | writes the label (at most 255 bytes) into every superblock copy, each with a fresh checksum; a pool of several devices is refused |
| `resize` | not implemented (exit 3): no resize |

`mkfs.btrfs` makes a single-device filesystem with the standard formatter's
defaults (metadata and system DUP, data single, the free-space tree, skinny
metadata, no-holes) on a device or image of at least 128 MiB:

```sh
mkfs.btrfs --size 1G -L BACKUP disk.img
truncate -s 4G disk.img && mkfs.btrfs -n 32768 --csum xxhash disk.img
```

It takes `-L`, `-n` (4 KiB to 64 KiB), `--csum` (crc32c, xxhash, sha256,
blake2), `-U`, `-f`, `-q` and `--size`, and accepts `-s 4096`, `-m dup`,
`-d single` and `-K`, which name what it makes. Any other sector size or profile
is refused by name rather than ignored. The layout is the standard formatter's,
measured from its output at sizes from 300 MiB to 300 GiB, written once at
generation 1; `tests/cli_mkfs_kernel.rs` has `btrfs check` and the kernel accept
it.

`fsck.btrfs` checks a volume without changing it, as `btrfs check --readonly`
does: every tree block's checksum and keys, the extent tree against the trees
that use the extents, block groups against their extents, chunks against device
extents, the free-space tree against the space nothing uses, and the namespace
(link counts, directory sizes, entries naming no inode).

```sh
fsck.btrfs disk.img          # exit 0 clean, 4 problems found, 8 could not check
fsck.btrfs --text disk.img   # one line per finding
```

It repairs nothing: `-y` and `-p` are refused (exit 16) rather than
accepted and ignored. `tests/cli_fsck_oracle.rs` holds it to `btrfs check` on
twelve clean fixtures and eight kinds of damage.

`--offset BYTES` addresses a volume inside a whole-disk image.

Every name has a man page (section 1; `man fs.btrfs`, `man fs.btrfs-ls`) and zsh,
bash and fish completions, written by the binary itself from the arguments it
parses (`rust-fs-btrfs generate man|completions SHARE`), so they cannot
describe a flag it does not take.

The release tarball, `rust-fs-btrfs-<version>-<platform>.tar.gz`, is an install
prefix: `bin/rust-fs-btrfs` and `bin/fs.btrfs` (a relative symlink to it),
`share/man/man1/`, the completions under `share/zsh/site-functions/`,
`share/bash-completion/completions/` and `share/fish/vendor_completions.d/`,
`share/rust-fs-btrfs/CAVEATS`, and `LICENSE`. rust-fs-core's `package-cli`
(`../rust-fs-core/scripts/package-cli.sh`), reading `[package.metadata.package-cli]` in
`Cargo.toml`, builds and checks it; CI builds it on every pull request.

`chore test:cli` tests the tools **as installed**, whatever PATH resolves:
`rust-fs-btrfs doctor` first, then `tests/cli/test-*.sh`, against a volume the
kernel populated in the harness VM (`test-disks/cli/`, with the kernel's own
manifest of every path, size and SHA-256). `tests/cli_oracle.rs` holds `get` to
`btrfs inspect-internal dump-super`, and checks that a volume `fs.btrfs` refuses
as damaged is one `btrfs check --readonly` refuses too; after `fs.btrfs write`,
`tests/cli_write_kernel.rs` has `btrfs check --readonly` find the volume clean
and the kernel read back exactly the bytes written.

[i262]: https://github.com/antimatter-studios/rust-fs-btrfs/issues/262
[i274]: https://github.com/antimatter-studios/rust-fs-btrfs/issues/274

## Test contract

Two layers, and only one of them can tell you the driver is right.

**Layer 1 — unit tests.** Fast, hermetic, no external tooling. They run on every
`cargo test` and prove the parser is *self-consistent*: that it accepts the
fixtures the crate builds for itself and reports the values those fixtures
encode.

That is a weaker claim than it looks. When a fixture is hand-built from the same
reading of the spec as the parser, a misread field is encoded wrong and decoded
wrong in exactly the same way, and the assertion passes. Byte-order slips,
transposed magic values, checksums computed over the wrong span — none of them
disturb a round-trip. A green unit suite means the driver is consistent with
itself, not that it reads Btrfs.

**Layer 2 — the real-kernel gate.** Real filesystems, built by the canonical
`mkfs.btrfs`, described by `btrfs inspect-internal dump-super`, checked by
`btrfs check`, and mounted by the in-kernel Btrfs driver. The driver then parses
those same images and must agree with the reference dump field by field.

This layer is **blocking in CI** (`.github/workflows/ci.yml`, gated by the
always-run `ci-ok` check), not an optional confirmation. The gate builds the full
fixture matrix, loop-mounts every image, writes and reads a file back through the
kernel, performs a whole transaction with this crate and has `btrfs check` and a
real mount judge the result, and fails if the kernel logs a single btrfs warning
— a mount that succeeds while the kernel complains is not a pass.

The case for making it blocking is empirical. In the sister
[XFS driver](https://github.com/antimatter-studios/rust-fs-xfs) the equivalent
gate found **three live parser bugs on its first run**, with the entire unit
suite green: a superblock magic with two bytes transposed, checksums stored
little-endian while every other field is big-endian, and a checksum computed over
the structure rather than the whole sector. Each is invisible to a round-trip
test and fatal against a real filesystem. There is no reason to expect Btrfs —
with more indirection, more checksum algorithms and more layout variation — to be
kinder.

**All of it happens inside the [fs-linux-test-harness][harness] VM**, and that is
the part worth knowing. Every `mkfs.btrfs`, every `btrfs check`, every `btrfs
inspect-internal` and every loop mount runs in one pinned Debian guest, on a
developer's machine exactly as in CI. It used to run on the CI runner under
`sudo`, which meant the kernel half of this gate could not be run on a Mac at
all, and on Linux only by handing a test suite root. The consequence you can see
from a laptop: `chore test` is the gate, wherever you are.

[harness]: https://github.com/antimatter-studios/fs-linux-test-harness

**Nothing skips.** A missing fixture, a missing tool or a VM that will not boot
fails the test that needed it, naming the task that provides it. That is not
tidiness either: this repository's leaf oracle once ran against no fixtures at
all and reported green, and the compression and nodatacow oracles found nothing
on every pull request for the same reason ([#69][i69]).

[i69]: https://github.com/antimatter-studios/rust-fs-btrfs/issues/69

### The geometry matrix

One list, in `test-disks/guest-build-images.sh`, built by one builder inside the
harness guest — so the gate a developer runs locally and the gate that guards the
branch cannot cover different ground. (There used to be two builders, and they
disagreed.)

| Fixture | `mkfs.btrfs` args | What it moves |
|---------|-------------------|---------------|
| `default` | — | the baseline the tooling picks for itself |
| `node4k` / `node16k` | `-n 4096` / `-n 16384` | every b-tree item offset |
| `csum-crc32c` | `--csum crc32c` | the classic checksum, 4 bytes used of 32 |
| `csum-xxhash` | `--csum xxhash` | 8-byte digest in a 32-byte field |
| `csum-sha256` | `--csum sha256` | full-width digest |
| `csum-blake2` | `--csum blake2` | full-width digest, different algorithm id |
| `single` | `-d single -m single` | explicit single profile, chunk-tree layout |
| `dup` | `-d dup -m dup` | duplicated data and metadata block groups |
| `mixed` | `-M` | data and metadata folded into one block-group type |

Each fixture is a ~400 MiB image written as `test-disks/btrfs-<name>.img` beside
its `test-disks/btrfs-<name>.superdump`. A geometry `mkfs.btrfs` refuses **fails
the build** — a fixture that quietly stopped being generated is a hole in the gate
that still reports green.

Beyond the matrix, the same builder produces the fixtures that need a *mount* to
populate: two filesystems filled until the fs tree is three levels deep, a
compressing mount per algorithm (zlib, LZO, zstd), subvolumes and snapshots,
extended attributes including a hash collision, a `chattr +C` file with no
checksums, a leaf split caught either side, a commit captured six times over, and
a two-device RAID1 pool. `test-disks/build-fixtures.sh --artefacts` lists every
file a full build produces.

## Running the gate

The same three commands on Linux and on macOS, because the Linux part happens in
the guest either way:

```sh
chore siblings   # ../rust-fs-core and ../fs-linux-test-harness, at their pinned refs
chore tools      # what the HOST needs: ripgrep, and the VM (Vagrant, QEMU, KVM/HVF)
chore fixtures   # every fixture, built by the real kernel inside the guest
chore test       # the whole gate, exactly as CI runs it
```

`chore tools` prints the exact install command for anything the host is missing.
On macOS `chore test` notices the host is not Linux and runs `chore test:vm`
instead — the same sources, compiled and run inside the guest.

### The tiers

`chore test` is the whole thing; each tier can be run on its own while working:

| Task | What it runs |
|------|--------------|
| `chore test:unit` | needs no tool, no fixture and no VM — the debug profile, which is the one that traps an arithmetic overflow |
| `chore test:images` | reads a fixture, needs no VM; this is what a machine without KVM can still run |
| `chore test:oracle` | the driver writes, btrfs-progs reads back, every tool call inside the guest |
| `chore test:kernel` | the driver writes, **the real kernel** reads back: our images loop-mounted in the guest |
| `chore test:vm` | the whole suite compiled and run inside the guest |
| `chore test:scripts` | the shell tests |

Which tier a test belongs to is derived from the test itself
(`scripts/test-targets.sh`), not from a list someone maintains: a suite is in the
gate the moment its file is committed. The previous arrangement named every suite
by hand in the workflow, and two suites that arrived after the list ran nowhere
at all ([#70][i70]).

[i70]: https://github.com/antimatter-studios/rust-fs-btrfs/issues/70

### Output

A task prints a verdict, not a transcript: a line per tier, a final count, and
the path of the log under `tmp/logs/` that holds everything else. A failing
tier is one line as well — its status and its log — unless
`OUTPUT_BUDGET_FAIL_TAIL=N` asks for the last N lines. `chore test --
--verbose` (or `OUTPUT_BUDGET_VERBOSE=1`) streams the whole run. Each tier
carries a **measured output budget**, and a tier that prints more than it is
allowed to fails the build (exit 65) — the table is at the top of `chores.yml`.

The runner and the wrapper enforcing that are `scripts/tier.sh` and
`scripts/output-budget.sh` from **rust-fs-core**, run in place from the sibling
checkout at the pinned version. They are not copied into this repository,
because a copy is something that drifts.

## Lint

```sh
chore lint       # cargo fmt --check, and clippy with -D warnings
```

## Building

The crate has a path dependency on the sibling `rust-fs-core` repository. Clone it
alongside this one:

```sh
git clone https://github.com/antimatter-studios/rust-fs-core.git ../rust-fs-core
cargo build --release
```

## Verifying a release

From the next release onward, every version published to crates.io is
also attached to the GitHub release for its tag, with a build-provenance
attestation signed by this repository's release workflow. It proves the
crate was built by `.github/workflows/release.yml` from a commit in this
repository, not uploaded from someone's machine. To check the crates.io
download of version `X.Y.Z`:

```sh
curl -sSfLo rust-fs-btrfs-X.Y.Z.crate https://static.crates.io/crates/rust-fs-btrfs/rust-fs-btrfs-X.Y.Z.crate
gh attestation verify rust-fs-btrfs-X.Y.Z.crate \
  --repo antimatter-studios/rust-fs-btrfs \
  --signer-workflow antimatter-studios/rust-fs-btrfs/.github/workflows/release.yml
```

The workflow refuses to attest a `.crate` whose sha256 differs from the
checksum crates.io records for that version, so the file on the release
page and the crates.io download are the same bytes.

## Changelog

The latest releases; every release, with the reasoning behind each change, is in [CHANGELOG.md](CHANGELOG.md).

### v0.10.2 — 2026-10-07

- The release's tarballs are packaged again, by rust-fs-core 0.3.7's release workflow.
- Depends on `rust-fs-core` 0.3.7.

### v0.10.1 — 2026-10-06

- The tools are released again.
- The family's scripts run in place from rust-fs-core 0.3.6.
- A release's notes are its CHANGELOG section.

### v0.10.0 — 2026-10-06

- Published as `rust-fs-btrfs`, the repository's name.
- Depends on `rust-fs-core` 0.3.0.

### v0.9.0 — 2026-10-06

- The last version published as `am-fs-btrfs`.
- `fsck.btrfs`, and `fs_btrfs::check` under it (#260).
- `mkfs.btrfs`, and `fs_btrfs::mkfs` under it (#259).
- `Error::InvalidGeometry`.

### v0.8.1 — 2026-10-03

- A release attaches the command-line tools.
- A copy-on-write file can be written (#61, first slice).
- The command-line plumbing comes from am-fs-core's `cli` feature.

### v0.8.0 — 2026-09-30

- In-image paths cross the C ABI as bytes, not UTF-8 (#214).
- `rust-fs-btrfs`, the command-line tools, as one multi-call binary.
- `fs.btrfs ls`, `read`, `get` and `info`.
- `fs.btrfs write`, for the one write this library can make.
- Man pages and shell completions, and the release tarball's layout.
- A transaction keeps each block group's `used` count true.
- A commit writes `ROOT_ITEM.generation_v2` with `generation`.
- A transaction touching a block group whose free-space records span two leaves is refused, instead of recording that free space twice.

### v0.7.0 — 2026-09-27

- `fs_btrfs_readlink` follows the family readlink contract.
- The parsers are fuzzed, on two tiers.
- A path can cross into subvolumes.
- Extended attributes can be read.
- Mounting no longer reads the whole filesystem tree.
- A lookup fetches the name by key.
- A read cannot follow an extent item's window outside its extent.
- A compressed stream that decodes short is refused anywhere but the file's last extent.
- A new tree block is never placed on a superblock copy.
- A commit fills its backup-root slot.
- An in-place write no longer overwrites data a snapshot still reads.

### v0.6.2 — 2026-09-06

- - A tree walk visits each block once. Without a visited set, a chunk

### v0.6.1 — 2026-09-04

- "Find a tree's root" means one thing.
- One definition of the endian readers.

### v0.6.0 — 2026-08-29

- A whole transaction is written and the kernel judges it.
- The free-space tree is kept in step.
- Pool reads: every device is opened and each mapping is answered from the right one.
- One device of a multi-device pool is refused rather than read as the whole thing.

## License

MIT — see [LICENSE](LICENSE).
