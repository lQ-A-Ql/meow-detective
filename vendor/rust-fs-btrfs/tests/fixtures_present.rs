//! Every fixture the build is supposed to produce is there.
//!
//! # Why this still exists now that nothing skips
//!
//! Each oracle now reaches its image through `fs_btrfs_test_support::fixture`,
//! which fails naming `chore fixtures` rather than printing a line and
//! returning — so a missing fixture can no longer be mistaken for a pass.
//! That closes the hole this file was written for, and it does it in the
//! place a new suite gets for free.
//!
//! What it does not do is fail FIRST and ALONE. A fixture build that
//! produced nothing at all makes several hundred tests fail at once, with
//! one sentence buried in each; this makes the cause its own failure,
//! before the suites that need the images run. `chore test:oracle` and
//! `chore test:kernel` call `test-disks/build-fixtures.sh --check` for
//! the same reason, and this is the copy that runs inside `cargo test` —
//! including inside the guest, where the shell check is a tier away.
//!
//! # Why an exact list and not a floor
//!
//! Because there is now one place that knows the list.
//! `test-disks/build-fixtures.sh --artefacts` prints every file a full
//! build produces, target by target, and `chore fixtures`' `generates:`
//! names the same images; a shell test (tests/scripts/fixture-builder.sh)
//! holds those two to each other. So this compares against the builder's
//! own answer rather than against a number somebody has to re-derive.
//!
//! The previous version of this file carried a hand-counted floor of 29
//! with a twenty-line comment explaining where the number came from, an
//! `AM_FIXTURES_REQUIRED` environment variable so it would not fail the
//! job that deliberately had no fixtures, and a note that an earlier
//! draft had taken the number from one developer's working directory.
//! All three of those are gone: the tiers say which jobs have fixtures,
//! and the builder says what a build produces.

use std::path::{Path, PathBuf};
use std::process::Command;

use fs_btrfs_test_support::fixture_dir;

/// What a full `chore fixtures` produces, asked of the builder itself.
fn expected() -> Vec<String> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("test-disks/build-fixtures.sh");
    assert!(
        script.is_file(),
        "{} is missing: it is the host side of the fixture build, and the one \
         place that knows what a build produces.",
        script.display()
    );
    let out = Command::new("bash")
        .arg(&script)
        .arg("--artefacts")
        .output()
        .unwrap_or_else(|error| panic!("cannot run {}: {error}", script.display()));
    assert!(
        out.status.success(),
        "{} --artefacts failed:\n{}",
        script.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let names: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    assert!(
        names.len() > 50,
        "the builder named only {} artefacts, which means the question was asked \
         of the wrong thing",
        names.len()
    );
    names
}

#[test]
fn the_fixture_build_produced_every_artefact_it_names() {
    let dir = fixture_dir();
    let missing: Vec<PathBuf> = expected()
        .into_iter()
        .map(|name| dir.join(name))
        .filter(|path| !path.is_file())
        .collect();

    assert!(
        missing.is_empty(),
        "{} of the fixtures are missing from {}:\n  {}\n\
         Build them with `chore fixtures`. Every one of them needs the real \
         kernel, so the build happens inside the fs-linux-test-harness VM; \
         `chore siblings` checks the harness out. Tests never skip on a \
         missing fixture, so this is the failure that names the cause once \
         rather than several hundred times.",
        missing.len(),
        dir.display(),
        missing
            .iter()
            .filter_map(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

/// Every image is a btrfs filesystem, checked here rather than taken on
/// the builder's word.
///
/// A truncated or half-copied image parses as far as "not btrfs" and then
/// fails somewhere else entirely — in whichever oracle happened to read it
/// first, with a message about that oracle. The magic is `_BHRfS_M` at
/// byte 0x40 of the superblock, which lives at 64 KiB.
#[test]
fn every_fixture_image_carries_the_btrfs_magic() {
    const MAGIC_AT: u64 = 0x1_0000 + 0x40;
    let dir = fixture_dir();
    let mut checked = 0;
    for name in expected() {
        if !name.ends_with(".img") {
            continue;
        }
        let path = dir.join(&name);
        let Ok(bytes) = std::fs::read(&path) else {
            // Absence is the other test's failure, not this one's: it
            // names all of them at once.
            continue;
        };
        let at = MAGIC_AT as usize;
        assert!(
            bytes.len() > at + 8,
            "{name} is {} bytes, which is shorter than one superblock",
            bytes.len()
        );
        assert_eq!(
            &bytes[at..at + 8],
            b"_BHRfS_M",
            "{name} has no btrfs superblock magic: it is not a btrfs filesystem, \
             so whichever oracle reads it first will fail about itself instead"
        );
        checked += 1;
    }
    assert!(
        checked > 0,
        "no fixture image was checked, so this compared nothing"
    );
    println!("{checked} fixture images carry the btrfs superblock magic");
}
