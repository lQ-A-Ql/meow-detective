//! rust-fs-btrfs — pure-Rust Btrfs filesystem driver.
//!
//! Exposes a stable C ABI (`fs_btrfs_*`) so FFI consumers (Swift/C/Go/…)
//! can link `libfs_btrfs.a` and `#include "fs_btrfs.h"`.
//!
//! # Byte order
//!
//! Btrfs is **little-endian on disk** on every host, checksums included.
//! Every on-disk integer in this crate is decoded with `from_le_bytes`.
//! That is worth stating up front because the sister XFS driver is
//! big-endian *except* for its CRC, and carrying either half of that
//! habit across produces a reader that agrees with its own test fixtures
//! and with nothing else.
//!
//! # Reading order
//!
//! Btrfs addresses everything — tree roots, node children, file extents —
//! in a single flat logical address space, and the table that translates
//! it lives inside the filesystem it describes. Bootstrapping therefore
//! runs in a fixed order:
//!
//! 1. Read the superblock from a fixed physical offset
//!    ([`superblock::SUPER_OFFSETS`]) and verify its checksum.
//! 2. Build the bootstrap address map from the superblock's embedded
//!    `sys_chunk_array` ([`chunk::ChunkMap::bootstrap`]).
//! 3. Use that map to reach the chunk tree, and extend the map with
//!    every chunk item it holds.
//! 4. Only then is `root` — and through it the rest of the volume —
//!    readable.
//!
//! All four steps are implemented, in [`fs::Filesystem::mount`].
//!
//! # Status
//!
//! Reads a volume -- directories, files including compressed extents,
//! symlinks, extended attributes, subvolumes, multi-device pools. It
//! writes file data in place to files marked nodatacow and nodatasum
//! ([`fs::Filesystem::write_at`]: no tree is rewritten and no generation
//! committed), and copy-on-write to files that are nodatasum but not
//! nodatacow ([`fs::Filesystem::write`], see [`cow_write`]): each extent
//! touched is copied into a new one and the change committed as a
//! transaction through the transaction and commit modules. Checksummed
//! files, growing a file and shared extents are still refused. Unit tests here are
//! deliberately treated as necessary-but-not-sufficient: a fixture this
//! crate builds itself cannot catch this crate misreading the on-disk
//! format, because the misreading would be baked into both sides.
//! Correctness against real media is established by cross-validating
//! against `mkfs.btrfs` output and the kernel's own writes.
//!
//! Architecture:
//! - [`error`] — driver error type, mapped to errno by the C ABI
//! - [`superblock`] — superblock parse + validation, checksum algorithms,
//!   feature gating; [`super_write`] re-stamps it for a commit
//! - [`chunk`] — chunk items and the logical-to-physical address map
//! - [`btree`] — tree blocks, descent and walks; [`leaf_edit`] and
//!   [`tree_write`] build new ones
//! - [`fs`] — the mounted volume: inodes, directories, file data;
//!   [`inode`], [`dir`], [`xattr`], [`subvol`], [`compression`] and
//!   [`csum`] each decode one kind of item
//! - [`write`](mod@write) — the in-place file write, which commits nothing
//! - [`cow_write`] — the copy-on-write file write, which commits a transaction
//! - [`transaction`], [`extent_write`], [`block_group`] and [`commit`] — tree
//!   rewrites and the commit of a new generation, separate from file writes
//! - [`capi`] — the C ABI

#![deny(unsafe_op_in_unsafe_fn)]

pub mod block_group;
pub mod btree;
pub mod capi;
pub mod check;
pub mod chunk;
pub mod commit;
pub mod compression;
pub mod cow_write;
pub mod csum;
pub mod dir;
pub mod error;
pub mod extent_write;
pub mod fs;
pub mod inode;
pub mod leaf_edit;
pub mod mkfs;
pub mod subvol;
pub mod super_write;
pub mod superblock;
pub mod transaction;
pub mod tree_write;
pub mod write;
pub mod xattr;

pub use chunk::{Chunk, ChunkMap, ChunkProfile, Mapping};
pub use error::{Error, Result};
pub use fs::Filesystem;
pub use superblock::{ChecksumType, Superblock};
pub use xattr::XattrEntry;

// DOES THIS BUILD ACTUALLY TRAP AN ARITHMETIC OVERFLOW?
//
// Inline in `lib.rs` rather than a module of its own under `src/`: a
// separate file hangs off one `mod` line, and losing that line leaves
// the file present, uncompiled and asserting nothing, with no lint to
// say so. That has already happened once on a sibling repository's
// version of this fix -- a `git reset --hard` took the declaration, the
// file stayed, and seven assertions quietly stopped existing. Inline,
// there is no declaration to lose. It cannot live in `tests/` either:
// the question it answers is about the library target that the debug
// run builds, so it has to be part of that target.
#[cfg(test)]
mod overflow_checks {
    /// Set by the `test:unit` task in `chores.yml`, and by nothing
    /// else. CI runs that task (the `unit` job) rather than a `cargo
    /// test` of its own, so the variable travels with the task and a
    /// workstation's `chore test:unit` arms this exactly as the gate
    /// does.
    ///
    /// The release runs must NOT set it: overflow checks are off there
    /// deliberately, because that is what ships. Setting it on a
    /// release run makes this module fail every time, which is the
    /// correct and loud response to that misconfiguration.
    const HANDSHAKE: &str = "EXPECT_OVERFLOW_CHECKS";

    /// Perform an overflow and report whether the program was stopped.
    ///
    /// The only question that matters, and the only one a text scan of
    /// `Cargo.toml` or the workflow cannot answer on its own: whichever
    /// spelling of "the checks are off" might exist -- a manifest key
    /// in any of its several spellings, a `CARGO_PROFILE_*` variable at
    /// step or job level, a `.cargo/config.toml` -- this asks the build
    /// directly instead of enumerating them and hoping the list is
    /// complete.
    fn this_build_traps_an_overflow() -> bool {
        // The hook is silenced so a deliberate panic does not print a
        // scary backtrace into a passing job's log.
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let trapped = std::panic::catch_unwind(|| {
            let big = std::hint::black_box(u64::MAX);
            std::hint::black_box(big + 1);
        })
        .is_err();
        std::panic::set_hook(previous);
        trapped
    }

    /// When the gate says it built a profile that traps, check that it
    /// did.
    ///
    /// With `HANDSHAKE` unset this asserts nothing -- the shape of a
    /// test that passes because its fixture is missing -- and that is
    /// not guarded here because it cannot be: a build has no way to
    /// know whether it was supposed to be the checking one. It is
    /// guarded in `tests/ci_profile.rs`, which follows the whole chain:
    /// `ci.yml` still runs `chore test:unit` on a job that gates a pull
    /// request, `chores.yml`'s `test:unit` still runs without
    /// `--release` while setting this variable, and
    /// `scripts/test-targets.sh unit` still names `--lib`. Any one of
    /// those three breaking is enough to leave this test armed by
    /// nothing.
    #[test]
    fn the_build_the_gate_asked_to_check_does_check() {
        let asked = match std::env::var(HANDSHAKE) {
            Ok(value) if !value.is_empty() => value,
            _ => return,
        };

        assert!(
            this_build_traps_an_overflow(),
            "{HANDSHAKE}={asked} was set, so this run is the one that is \
             supposed to panic on arithmetic overflow -- and it did not. \
             The debug step is running and blind, which is the exact \
             state it exists to rule out."
        );
    }
}
