//! Compressed files must come back exactly as the kernel wrote them.
//!
//! Btrfs defines three compression algorithms and they share no code: a
//! reader can have zstd perfectly right and LZO silently wrong. Each one
//! therefore gets its own fixture, written through a mount using that
//! algorithm, with a manifest generated **inside Linux by the kernel's
//! own driver** — path, size and SHA-256 per file. Nothing in this
//! repository decides what the right answer is.
//!
//! LZO is the reason this is a separate suite rather than one more case
//! in `fs_oracle`. zlib and zstd extents are ordinary streams of their
//! respective formats, and any correct decoder reads them. LZO1X has no
//! container at all, so Btrfs wraps it in framing of its own invention:
//! a total length, then per-sector segments, with a segment header never
//! allowed to straddle a sector boundary. That padding rule is invisible
//! in any file short enough to fit in one sector — which is every file a
//! hand-written test is likely to build — so the fixture deliberately
//! includes one large enough to need several.
//!
//! The fixtures are gitignored and generated: `chore fixtures` builds
//! them inside the fs-linux-test-harness VM, where the kernel's own
//! compressors write them. Nothing here skips — a missing image fails
//! the test that wanted it, naming the task that builds it, because a
//! decoder suite that ran against no fixture reports `ok` exactly like
//! one that decoded everything correctly.

use fs_btrfs::Filesystem;
use fs_btrfs_test_support::{fixture, sha256_hex};
use fs_core::FileDevice;
use std::path::PathBuf;
use std::sync::Arc;

/// One manifest line: what the kernel says a file holds.
struct Expected {
    path: String,
    size: u64,
    sha256: String,
}

/// Every algorithm's fixture, as (algorithm, image, manifest).
///
/// Named one by one rather than walked as `btrfs-comp-*`, because the
/// three decoders share no code: a run that found only zstd built would
/// prove nothing about LZO and still be a full list. All three are
/// required, and `fixture` is what enforces it.
fn fixtures() -> Vec<(String, PathBuf, PathBuf)> {
    ["zlib", "lzo", "zstd"]
        .into_iter()
        .map(|algo| {
            (
                algo.to_string(),
                fixture(&format!("btrfs-comp-{algo}.img")),
                fixture(&format!("btrfs-comp-{algo}.manifest")),
            )
        })
        .collect()
}

fn parse_manifest(text: &str) -> Vec<Expected> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            Some(Expected {
                path: f.next()?.to_string(),
                size: f.next()?.parse().ok()?,
                sha256: f.next()?.to_string(),
            })
        })
        .collect()
}

/// Every file in every per-algorithm fixture must read back byte for
/// byte, judged by the hash the kernel computed for it.
#[test]
fn compressed_files_match_what_the_kernel_wrote() {
    let fixtures = fixtures();

    let mut checked = 0usize;
    for (algo, img, manifest_path) in &fixtures {
        let expected = parse_manifest(&std::fs::read_to_string(manifest_path).expect("manifest"));
        assert!(!expected.is_empty(), "{algo}: manifest is empty");

        let dev = FileDevice::open(img).unwrap_or_else(|e| panic!("{algo}: open: {e}"));
        let fs = Filesystem::mount(Arc::new(dev)).unwrap_or_else(|e| panic!("{algo}: mount: {e}"));

        for want in &expected {
            let inode = fs
                .lookup_path(&want.path)
                .unwrap_or_else(|e| panic!("{algo}: {} not found: {e}", want.path));
            let got = fs
                .read_file(inode.ino)
                .unwrap_or_else(|e| panic!("{algo}: {} failed to read: {e}", want.path));

            assert_eq!(
                got.len() as u64,
                want.size,
                "{algo}: {} is {} bytes, the kernel says {}",
                want.path,
                got.len(),
                want.size
            );
            assert_eq!(
                sha256_hex(&got),
                want.sha256,
                "{algo}: {} decoded to different bytes than the kernel wrote",
                want.path
            );
            checked += 1;
        }
        eprintln!("{algo}: {} files match", expected.len());
    }
    assert!(checked > 0, "no files were compared");
}

/// The fixtures must actually contain the algorithm they are named for.
///
/// A mount option Btrfs decided to ignore, or a kernel that fell back to
/// storing a file uncompressed, would leave the suite above passing
/// while testing none of the decoders. So the builder records the
/// compression types the reference tool found on disk, and this requires
/// the expected one to be among them.
#[test]
fn each_fixture_really_uses_its_algorithm() {
    let fixtures = fixtures();
    for (algo, img, _) in &fixtures {
        let record = img.with_extension("compression");
        let text = std::fs::read_to_string(&record).unwrap_or_else(|_| {
            panic!(
                "{algo}: no {} — rebuild the fixtures with `chore fixtures`",
                record.display()
            )
        });
        assert!(
            text.contains(&format!("({algo})")),
            "{algo}: nothing on this filesystem is {algo}-compressed, so the fixture \
             exercises none of that decoder. The reference tool found:\n{text}"
        );
        // And the incompressible file must have stayed plain, or a driver
        // that mangled every uncompressed read could still pass.
        assert!(
            text.contains("(none)"),
            "{algo}: every extent is compressed, so nothing checks the plain path:\n{text}"
        );
    }
}

/// The large fixture file must be big enough to span several sectors.
///
/// This is what makes the LZO segment framing observable. A future
/// change that shrinks the fixture would leave the suite green while
/// quietly dropping the only coverage of the padding rule.
#[test]
fn the_large_fixture_spans_several_sectors() {
    let fixtures = fixtures();
    for (algo, _, manifest_path) in &fixtures {
        let expected = parse_manifest(&std::fs::read_to_string(manifest_path).expect("manifest"));
        let big = expected
            .iter()
            .find(|e| e.path == "/big.txt")
            .unwrap_or_else(|| panic!("{algo}: the fixture has no /big.txt"));
        // Sectors are 4 KiB by default; several of them, with room to
        // spare, so this does not become brittle if that changes.
        assert!(
            big.size > 64 * 1024,
            "{algo}: /big.txt is only {} bytes, too small to span the sectors \
             the LZO framing pads between",
            big.size
        );
    }
}
