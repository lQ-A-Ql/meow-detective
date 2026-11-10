//! Cross-validation against filesystems built by real Btrfs tooling.
//!
//! The unit tests in `src/` parse structures this crate built itself.
//! That proves the parser is self-consistent; it cannot prove it reads
//! Btrfs the way the rest of the world reads it, because a misreading of
//! the on-disk format is baked into both the fixture and the parser and
//! they agree with each other while disagreeing with reality.
//!
//! These tests close that gap. `mkfs.btrfs` builds the filesystems and
//! `btrfs inspect-internal dump-super` reports what the reference
//! tooling believes each field to be — both in the fs-linux-test-harness
//! VM, when `chore fixtures` builds them — and this driver must agree
//! field by field.
//!
//! This is not a theoretical concern. In the sibling XFS crate three bugs
//! survived a fully green unit suite — a transposed magic constant, a
//! checksum field stored in a different endianness from the rest of the
//! format, and a checksum covering a whole sector rather than a struct.
//! All three died on the first comparison against the reference debugger.
//!
//! THE COMPARISON NEEDS NO VM, and that is deliberate: the dump-super
//! report travels with the image it describes, as
//! `test-disks/btrfs-<name>.superdump` beside `btrfs-<name>.img`, so a
//! host with no KVM still runs the whole of this file. Both halves are
//! fixtures, both come from `chore fixtures`, and a missing one fails
//! here naming the task that builds it.

use fs_btrfs::superblock::{Superblock, SUPER_INFO_OFFSET};
use fs_btrfs_test_support::{fixtures_matching, Image};
use std::collections::HashMap;
use std::path::PathBuf;

/// `BTRFS_FEATURE_INCOMPAT_METADATA_UUID`. When clear, the on-disk
/// metadata_uuid field is unused and the effective value is the fsid.
const METADATA_UUID_INCOMPAT: u64 = 1 << 10;

/// One parsed line of a `dump-super` report.
///
/// The tool prints `key<TAB>value`, with flag words followed by
/// indented parenthesised expansions that carry no `key` and are
/// ignored here.
#[derive(Default)]
struct Dump {
    nums: HashMap<String, u64>,
    strs: HashMap<String, String>,
}

impl Dump {
    fn parse(text: &str) -> Self {
        let mut d = Dump::default();
        for line in text.lines() {
            let line = line.trim_end();
            // Flag expansions are indented continuation lines.
            if line.starts_with(char::is_whitespace) || line.is_empty() {
                continue;
            }
            let Some((k, v)) = line.split_once(char::is_whitespace) else {
                continue;
            };
            let k = k.trim();
            let mut v = v.trim();
            if k.is_empty() || v.is_empty() {
                continue;
            }
            // "0x9094899c [match]" -> "0x9094899c"
            if let Some(sp) = v.find(" [") {
                v = &v[..sp];
            }
            // "0 (crc32c)" -> "0"
            if let Some(sp) = v.find(" (") {
                v = &v[..sp];
            }
            d.strs.insert(k.to_string(), v.to_string());
            let n = if let Some(hex) = v.strip_prefix("0x") {
                u64::from_str_radix(hex, 16).ok()
            } else {
                v.parse::<u64>().ok()
            };
            if let Some(n) = n {
                d.nums.insert(k.to_string(), n);
            }
        }
        d
    }
}

/// Every fixture image paired with the dump-super report taken from it,
/// in the sorted order `fixtures_matching` returns.
///
/// A handful of images are built without a report — the before/after
/// pairs and the two halves of the pool, which exist to be compared with
/// each other rather than with the reference tooling — so they are named
/// rather than quietly dropped, and an empty pairing is a failure.
fn fixtures() -> Vec<(String, PathBuf, PathBuf)> {
    let mut out = Vec::new();
    let mut undumped = Vec::new();
    for img in fixtures_matching("btrfs-") {
        let name = img.file_stem().unwrap().to_string_lossy().into_owned();
        let dump = img.with_extension("superdump");
        if dump.is_file() {
            out.push((name, img, dump));
        } else {
            undumped.push(name);
        }
    }
    if !undumped.is_empty() {
        eprintln!(
            "  {} image(s) carry no dump-super report: {undumped:?}",
            undumped.len()
        );
    }
    assert!(
        !out.is_empty(),
        "no btrfs-*.superdump reports in test-disks/, so there is nothing to compare \
         this driver against. `chore fixtures` builds them beside the images."
    );
    out
}

fn expect(d: &Dump, field: &str, ours: u64, label: &str, checked: &mut usize) {
    let Some(&theirs) = d.nums.get(field) else {
        // Say so rather than passing silently: an unnoticed skip is a
        // hole in the gate.
        eprintln!("  {label}: dump-super did not report `{field}` — not compared");
        return;
    };
    assert_eq!(
        ours, theirs,
        "{label}: field `{field}` — this driver says {ours}, dump-super says {theirs}"
    );
    *checked += 1;
}

/// Render a UUID the way the reference tooling prints it.
fn uuid_string(u: &[u8; 16]) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        u[0], u[1], u[2], u[3], u[4], u[5], u[6], u[7],
        u[8], u[9], u[10], u[11], u[12], u[13], u[14], u[15]
    )
}

/// Named for the report rather than for the tool, and deliberately not
/// spelled the way the helper is called: `scripts/test-targets.sh` sorts
/// the suite by grepping for that call, so a name matching it would put
/// this file in the tier that needs a VM. It does not need one — it
/// compares against the transcript the fixture build recorded beside
/// each image, which is what keeps this cross-validation running on a
/// machine with no KVM.
#[test]
fn superblock_agrees_with_the_dump_super_report() {
    let fixtures = fixtures();

    let mut total = 0usize;
    for (label, img, dump_path) in &fixtures {
        // The primary superblock lives at 64 KiB, not at offset 0.
        // parse_at additionally requires the copy to agree it belongs
        // there, which catches a stale-but-intact superblock left by an
        // earlier filesystem at a different mirror offset.
        let head = Image::open(img).read_at(SUPER_INFO_OFFSET, 4096);
        let sb = Superblock::parse_at(&head, SUPER_INFO_OFFSET)
            .unwrap_or_else(|e| panic!("{label}: failed to parse a real filesystem: {e}"));
        let d = Dump::parse(&std::fs::read_to_string(dump_path).expect("read superdump"));
        assert!(!d.nums.is_empty(), "{label}: superdump held no fields");

        let mut checked = 0usize;
        expect(&d, "bytenr", sb.bytenr, label, &mut checked);
        expect(&d, "flags", sb.flags, label, &mut checked);
        expect(&d, "generation", sb.generation, label, &mut checked);
        expect(&d, "root", sb.root, label, &mut checked);
        expect(&d, "chunk_root", sb.chunk_root, label, &mut checked);
        expect(&d, "log_root", sb.log_root, label, &mut checked);
        expect(&d, "total_bytes", sb.total_bytes, label, &mut checked);
        expect(&d, "bytes_used", sb.bytes_used, label, &mut checked);
        expect(&d, "num_devices", sb.num_devices, label, &mut checked);
        expect(&d, "root_dir", sb.root_dir_objectid, label, &mut checked);
        expect(&d, "sectorsize", sb.sectorsize.into(), label, &mut checked);
        expect(&d, "nodesize", sb.nodesize.into(), label, &mut checked);
        expect(&d, "stripesize", sb.stripesize.into(), label, &mut checked);
        expect(
            &d,
            "sys_array_size",
            sb.sys_chunk_array_size.into(),
            label,
            &mut checked,
        );
        expect(
            &d,
            "chunk_root_generation",
            sb.chunk_root_generation,
            label,
            &mut checked,
        );
        expect(&d, "root_level", sb.root_level.into(), label, &mut checked);
        expect(
            &d,
            "chunk_root_level",
            sb.chunk_root_level.into(),
            label,
            &mut checked,
        );
        expect(
            &d,
            "log_root_level",
            sb.log_root_level.into(),
            label,
            &mut checked,
        );
        expect(&d, "compat_flags", sb.compat_flags, label, &mut checked);
        expect(
            &d,
            "compat_ro_flags",
            sb.compat_ro_flags,
            label,
            &mut checked,
        );
        expect(&d, "incompat_flags", sb.incompat_flags, label, &mut checked);
        expect(
            &d,
            "csum_size",
            sb.csum_type.digest_len() as u64,
            label,
            &mut checked,
        );

        // UUIDs are the decisive check on the least certain offsets in
        // this parser: metadata_uuid in particular is absent from the
        // published format tables and was derived from field ordering.
        if let Some(theirs) = d.strs.get("fsid") {
            assert_eq!(
                &uuid_string(&sb.fsid),
                theirs,
                "{label}: fsid disagrees with dump-super"
            );
            checked += 1;
        }
        // metadata_uuid needs care, because the reference tooling changed
        // what it prints. When the METADATA_UUID feature is off, the
        // on-disk field is all zeros and the effective metadata UUID is
        // simply the fsid. btrfs-progs 6.2 printed the effective value;
        // 6.6.3 prints the raw zeros. Both describe the same state, so
        // accept either and pin down the part that actually matters:
        // this driver must report the EFFECTIVE uuid, since it is what
        // tree node headers are stamped with and what their identity
        // checks are compared against.
        if let Some(theirs) = d.strs.get("metadata_uuid") {
            const ALL_ZERO: &str = "00000000-0000-0000-0000-000000000000";
            let feature_on = d
                .nums
                .get("incompat_flags")
                .is_some_and(|f| f & METADATA_UUID_INCOMPAT != 0);
            let ours = uuid_string(&sb.metadata_uuid);
            if feature_on {
                assert_eq!(
                    &ours, theirs,
                    "{label}: the METADATA_UUID feature is set, so the driver and \
                     dump-super must agree exactly on metadata_uuid"
                );
            } else {
                assert_eq!(
                    ours,
                    uuid_string(&sb.fsid),
                    "{label}: with METADATA_UUID off the effective metadata uuid is \
                     the fsid; this driver reported something else"
                );
                assert!(
                    theirs == ALL_ZERO || theirs == &ours,
                    "{label}: dump-super reported metadata_uuid {theirs}, which is \
                     neither the raw zeros nor the fsid"
                );
            }
            checked += 1;
        }

        assert!(
            checked >= 15,
            "{label}: only {checked} fields compared — not enough to call this validated"
        );
        eprintln!("  {label}: {checked} fields agree with dump-super");
        total += checked;
    }
    eprintln!(
        "{} fixtures, {total} field comparisons against dump-super",
        fixtures.len()
    );
}

/// Every checksum algorithm must verify against real media. This is the
/// exact class of bug that bit the sibling XFS crate — a checksum read
/// or computed the wrong way passes hand-built fixtures and fails on
/// every real filesystem.
#[test]
fn every_checksum_algorithm_verifies_on_real_media() {
    let fixtures = fixtures();
    let mut seen = Vec::new();
    for (label, img, dump_path) in &fixtures {
        // Parsing succeeding IS the checksum check: the parser verifies
        // the superblock checksum and refuses a mismatch.
        let head = Image::open(img).read_at(SUPER_INFO_OFFSET, 4096);
        let sb = Superblock::parse_at(&head, SUPER_INFO_OFFSET)
            .unwrap_or_else(|e| panic!("{label}: checksum verification failed: {e}"));
        let d = Dump::parse(&std::fs::read_to_string(dump_path).unwrap());
        if let Some(theirs) = d.nums.get("csum_type") {
            assert_eq!(
                u64::from(sb.csum_type.to_raw()),
                *theirs,
                "{label}: csum_type disagrees with dump-super"
            );
        }
        seen.push(format!("{label}={:?}", sb.csum_type));
    }
    eprintln!("checksum algorithms verified: {}", seen.join(", "));
    // The fixture matrix builds one filesystem per algorithm. If only one
    // distinct algorithm was exercised, the matrix is not doing its job.
    let distinct: std::collections::HashSet<_> = seen
        .iter()
        .map(|s| s.split('=').nth(1).unwrap().to_string())
        .collect();
    assert!(
        distinct.len() >= 2,
        "only one checksum algorithm exercised ({distinct:?}) — the fixture \
         matrix is supposed to cover crc32c, xxhash, sha256 and blake2"
    );
}

/// The chunk tree bootstrap is the gate on everything else in Btrfs:
/// until logical addresses can be translated to physical ones, no tree
/// can be read. Prove it works against real media by translating the
/// chunk root the superblock points at.
#[test]
fn chunk_bootstrap_maps_chunk_root_on_real_media() {
    let fixtures = fixtures();
    for (label, img, _) in &fixtures {
        let image = Image::open(img);
        let head = image.read_at(SUPER_INFO_OFFSET, 4096);
        let sb = Superblock::parse_at(&head, SUPER_INFO_OFFSET).expect("parse superblock");
        let map = fs_btrfs::chunk::ChunkMap::bootstrap(&sb)
            .unwrap_or_else(|e| panic!("{label}: chunk bootstrap failed on real media: {e}"));

        let mapping = map
            .map(sb.chunk_root)
            .unwrap_or_else(|e| panic!("{label}: chunk_root {} is unmapped: {e}", sb.chunk_root));
        let phys = mapping.physical;

        assert!(
            phys < sb.total_bytes,
            "{label}: chunk_root maps to {phys}, past the {}-byte device",
            sb.total_bytes
        );

        // The block the mapping lands on must actually be a node header
        // carrying this filesystem's identity. A mapping that is merely
        // in range proves nothing.
        let mut node = vec![0u8; sb.nodesize as usize];
        assert!(
            image.try_read_at(phys, &mut node),
            "{label}: chunk_root node is off the end"
        );
        let node_fsid = &node[32..48];
        assert_eq!(
            node_fsid,
            &sb.fsid[..],
            "{label}: the block chunk_root maps to does not carry this filesystem's \
             fsid — the logical-to-physical translation is wrong"
        );
        eprintln!("  {label}: chunk_root {} -> physical {phys}", sb.chunk_root);
    }
}
