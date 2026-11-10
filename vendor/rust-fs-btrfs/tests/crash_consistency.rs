//! What survives an interruption: a commit of this crate cut at every
//! point it can be cut, and a log tree the kernel wrote and never
//! replayed. THE KERNEL DECIDES, and `btrfs check` beside it.
//!
//! # The commit, stopped halfway
//!
//! `tests/commit_order.rs` pins the order a commit reaches the device in
//! — tree blocks (every mirror), flush, superblocks, flush — and
//! `src/commit.rs` explains why that order is the crash-consistency. That
//! is reasoning about the property. This tests it: a real transaction of
//! this crate is recorded write by write, and every image a power cut
//! could leave behind is handed to the in-kernel driver and to the
//! checker. Each must either mount and show EXACTLY the filesystem before
//! the commit or EXACTLY the one after it, or refuse. Anything in between
//! — a mount that returns some of the new bytes, a checker that finds a
//! dangling pointer — is the failure the ordering exists to prevent.
//!
//! The cuts: after each tree-block write, with half of one torn, with the
//! device's volatile cache dropping one before the barrier; after each
//! superblock copy; with the primary copy torn; and with the primary
//! dropped while a later copy survived, which a volatile cache is free to
//! do between the two flushes.
//!
//! The transaction relocates the fs tree and changes one file's inline
//! bytes in the relocated leaf, so "before" and "after" differ in a way
//! a mount can see. Both are read off the kernel — the untouched image
//! and the fully committed one — rather than assumed, and every cut is
//! compared with those two.
//!
//! # The log tree nobody replayed
//!
//! `fsync` on btrfs writes a log tree and points the superblock at it;
//! the next mount replays it. Until then the committed trees hold the
//! file as it was BEFORE the fsync, so a reader that ignored `log_root`
//! would hand back exactly the bytes an application was told were
//! durably replaced. This crate does not replay a log; it refuses the
//! mount. The fixture is a kernel image copied while the log was live,
//! and the test pins the refusal, shows what ignoring the log would have
//! returned, and has the kernel replay it for comparison.
//!
//! This crate writes no log tree, so "a log tree this crate wrote,
//! replayed by the kernel" has no subject here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use fs_btrfs::btree::header_offsets;
use fs_btrfs::chunk::objectid;
use fs_btrfs::error::Error;
use fs_btrfs::fs::Filesystem;
use fs_btrfs::inode::FileType;
use fs_btrfs::super_write::Commit;
use fs_btrfs::superblock::Superblock;
use fs_btrfs_test_support::{
    dump_super, fixture, guest_kernel_read_variants, guest_kernel_write_ok, le64, sha256_hex,
    temp_path, Variant, VariantVerdict,
};
use fs_core::{BlockDevice, BlockRead, FileDevice, Result as CoreResult};

/// The superblock copies, by byte offset. Hand-written rather than taken
/// from the crate: the offsets are what the cuts are made at, and
/// borrowing the reader's idea of them would test it against itself.
const SUPER_COPIES: [u64; 3] = [0x1_0000, 0x400_0000, 0x40_0000_0000];
/// One superblock copy.
const SUPER_LEN: usize = 4096;
/// `log_root` and `log_root_level` within a superblock copy.
const LOG_ROOT: usize = 0x60;
const LOG_ROOT_LEVEL: usize = 0xc8;

/// What the device was asked to do, in order.
#[derive(Clone)]
enum Op {
    Write(u64, Vec<u8>),
    Flush,
}

/// A fixture read through, with writes laid over it in memory and every
/// write and flush recorded in order.
///
/// Never touches the fixture: the images here are half a gigabyte, and
/// the variants differ from it by a few blocks, so they are overlays and
/// not copies.
struct Overlay {
    base: FileDevice,
    patches: Mutex<Vec<(u64, Vec<u8>)>>,
    ops: Mutex<Vec<Op>>,
}

impl Overlay {
    fn new(base: &Path, patches: Vec<(u64, Vec<u8>)>) -> Arc<Self> {
        Arc::new(Overlay {
            base: FileDevice::open(base)
                .unwrap_or_else(|error| panic!("opening {}: {error}", base.display())),
            patches: Mutex::new(patches),
            ops: Mutex::new(Vec::new()),
        })
    }
}

impl BlockRead for Overlay {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> CoreResult<()> {
        self.base.read_at(offset, buf)?;
        let end = offset + buf.len() as u64;
        for (at, bytes) in self.patches.lock().unwrap().iter() {
            let (from, to) = ((*at).max(offset), (at + bytes.len() as u64).min(end));
            if from < to {
                buf[(from - offset) as usize..(to - offset) as usize]
                    .copy_from_slice(&bytes[(from - at) as usize..(to - at) as usize]);
            }
        }
        Ok(())
    }
    fn size_bytes(&self) -> u64 {
        self.base.size_bytes()
    }
}

impl BlockDevice for Overlay {
    fn write_at(&self, offset: u64, buf: &[u8]) -> CoreResult<()> {
        self.ops
            .lock()
            .unwrap()
            .push(Op::Write(offset, buf.to_vec()));
        self.patches.lock().unwrap().push((offset, buf.to_vec()));
        Ok(())
    }
    fn flush(&self) -> CoreResult<()> {
        self.ops.lock().unwrap().push(Op::Flush);
        Ok(())
    }
    fn is_writable(&self) -> bool {
        true
    }
}

/// Every regular file in the top directory, by name, as a SHA-256 — what
/// the kernel script below prints, read through this crate instead.
fn crate_report(fs: &Filesystem) -> Result<BTreeMap<String, String>, Error> {
    let mut out = BTreeMap::new();
    for entry in fs.list_path("/")? {
        if entry.ftype != Some(FileType::Regular) {
            continue;
        }
        let name = String::from_utf8_lossy(&entry.name).into_owned();
        let bytes = fs.read_path(&format!("/{name}"))?;
        out.insert(name, sha256_hex(&bytes));
    }
    Ok(out)
}

/// The same, in the guest, against `$MNT`. A file the kernel cannot
/// read fails the script, and a mount whose files cannot be read is not
/// a clean mount.
const KERNEL_REPORT: &str = r#"
cd "$MNT"
find . -maxdepth 1 -type f -printf '%P\n' | sort | while read -r f; do
    printf '%s\t%s\n' "$f" "$(sha256sum < "$f" | cut -d' ' -f1)"
done
"#;

fn parse_report(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(name, hash)| (name.to_string(), hash.to_string()))
        .collect()
}

/// Every line of a `btrfs check` report that says it did not check
/// something — except the qgroup pass on a filesystem with no qgroups,
/// which has nothing to check. Anything else skipped means the verdict
/// covers less than the image, and is not a pass.
fn skipped(report: &str) -> Vec<&str> {
    report
        .lines()
        .filter(|line| line.to_lowercase().contains("skip"))
        .filter(|line| !line.contains("checking quota groups skipped (not enabled on this FS)"))
        .collect()
}

/// Which side of the commit a view of the filesystem is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Before,
    After,
}

fn classify(
    seen: &BTreeMap<String, String>,
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) -> Option<State> {
    if seen == before {
        Some(State::Before)
    } else if seen == after {
        Some(State::After)
    } else {
        None
    }
}

/// The file whose inline bytes the transaction changes, and to what.
const CHANGED_FILE: &str = "file-1.txt";
const OLD_BYTES: &[u8] = b"commit 1\n";
const NEW_BYTES: &[u8] = b"COMMIT 1\n";

/// Perform one transaction on `image` through a recording device and
/// return the writes and flushes it made, in order.
///
/// The shape is `tests/kernel_readback.rs`'s, with the fs tree as the
/// dirty block rather than the root tree, so the relocated leaf is the
/// one holding the files — and one file's inline data changed in it,
/// checksum restamped, so the commit changes something a mount can see.
fn record_commit(image: &Path, label: &str) -> Vec<Op> {
    let dev = Overlay::new(image, Vec::new());
    let fs = Filesystem::mount_rw(dev.clone() as Arc<dyn BlockDevice>)
        .unwrap_or_else(|error| panic!("[{label}] mounting read-write: {error}"));
    let generation = fs.superblock().generation + 1;

    const ROOT_ITEM_KEY: u8 = 132;
    const ROOT_ITEM_BYTENR: usize = 176;
    let fs_root = fs
        .root_tree_items()
        .expect("reading the root tree")
        .into_iter()
        .find_map(|(id, ty, _, data)| {
            (id == objectid::FS_TREE && ty == ROOT_ITEM_KEY && data.len() >= ROOT_ITEM_BYTENR + 8)
                .then(|| le64(&data, ROOT_ITEM_BYTENR))
        })
        .expect("the root tree names the fs tree");

    let plan = fs
        .plan_transaction_closed(&[fs_root], 8)
        .unwrap_or_else(|error| panic!("[{label}] planning: {error}"));
    let mut blocks = fs
        .render_plan(&plan, generation)
        .unwrap_or_else(|error| panic!("[{label}] rendering: {error}"));
    let new_root = fs
        .planned_root(&plan)
        .expect("moving the fs tree moves the root tree leaf naming it");

    // The one visible change: the inline extent's bytes, in the fs tree
    // leaf the plan relocated. Same length, uncompressed, and inline data
    // carries no checksum of its own, so the leaf checksum is the only
    // other thing that moves.
    let mut changed = 0;
    for block in &mut blocks {
        if le64(&block.bytes, header_offsets::OWNER) != objectid::FS_TREE {
            continue;
        }
        let hits: Vec<usize> = block
            .bytes
            .windows(OLD_BYTES.len())
            .enumerate()
            .filter(|(_, w)| *w == OLD_BYTES)
            .map(|(at, _)| at)
            .collect();
        for at in &hits {
            block.bytes[*at..*at + NEW_BYTES.len()].copy_from_slice(NEW_BYTES);
        }
        if !hits.is_empty() {
            fs_btrfs::tree_write::stamp_checksum(&mut block.bytes, fs.superblock());
        }
        changed += hits.len();
    }
    assert_eq!(
        changed, 1,
        "[{label}] expected {CHANGED_FILE}'s inline bytes exactly once in the relocated fs tree"
    );

    fs.commit(
        &blocks,
        &Commit {
            generation,
            root: new_root,
            root_level: None,
            bytes_used: None,
            chunk_root: None,
            chunk_root_generation: None,
            invalidate_free_space_tree: true,
        },
    )
    .unwrap_or_else(|error| panic!("[{label}] committing: {error}"));
    let ops = dev.ops.lock().unwrap().clone();
    ops
}

/// One place a power cut can leave the device, and what each reader
/// should make of it.
struct Cut {
    label: String,
    patches: Vec<(u64, Vec<u8>)>,
    /// The primary superblock copy reached the device whole.
    primary_new: bool,
    /// The primary copy reached it torn.
    primary_torn: bool,
    /// Some later copy reached it whole.
    mirror_new: bool,
}

/// Every cut worth making in `ops`: the tree writes before the first
/// flush, then the superblock copies between the two.
fn cuts(ops: &[Op]) -> Vec<Cut> {
    let flushes: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, op)| matches!(op, Op::Flush))
        .map(|(at, _)| at)
        .collect();
    assert_eq!(
        flushes.len(),
        2,
        "a commit is two flushes; the order tests/commit_order.rs pins has changed"
    );
    let writes = |range: std::ops::Range<usize>| -> Vec<(u64, Vec<u8>)> {
        ops[range]
            .iter()
            .filter_map(|op| match op {
                Op::Write(at, bytes) => Some((*at, bytes.clone())),
                Op::Flush => None,
            })
            .collect()
    };
    let tree = writes(0..flushes[0]);
    let supers = writes(flushes[0] + 1..flushes[1]);
    assert!(
        !tree.is_empty(),
        "the commit wrote no tree blocks, so there is nothing to cut"
    );
    assert!(
        supers.len() >= 2 && supers[0].0 == SUPER_COPIES[0],
        "the superblocks are not copy 0 then the mirrors: {:?}",
        supers.iter().map(|(at, _)| at).collect::<Vec<_>>()
    );
    assert!(
        supers
            .iter()
            .all(|(at, b)| SUPER_COPIES.contains(at) && b.len() == SUPER_LEN),
        "a write between the flushes is not a whole superblock copy"
    );

    let mut out = Vec::new();
    let mut push = |label: String, patches: Vec<(u64, Vec<u8>)>| {
        let whole = |copy: u64| {
            patches
                .iter()
                .any(|(at, b)| *at == copy && b.len() == SUPER_LEN)
        };
        out.push(Cut {
            primary_new: whole(SUPER_COPIES[0]),
            primary_torn: patches
                .iter()
                .any(|(at, b)| *at == SUPER_COPIES[0] && b.len() < SUPER_LEN),
            mirror_new: SUPER_COPIES[1..].iter().any(|&copy| whole(copy)),
            label,
            patches,
        });
    };

    push("nothing written".into(), Vec::new());
    for k in 1..=tree.len() {
        push(
            format!("{k} of {} tree-block writes, no barrier", tree.len()),
            tree[..k].to_vec(),
        );
    }
    let (at, bytes) = &tree[0];
    push(
        "the first tree-block write torn in half".into(),
        vec![(*at, bytes[..bytes.len() / 2].to_vec())],
    );
    if tree.len() > 1 {
        push(
            "every tree-block write but the first (lost from the cache)".into(),
            tree[1..].to_vec(),
        );
    }
    for j in 1..=supers.len() {
        let mut patches = tree.clone();
        patches.extend(supers[..j].iter().cloned());
        push(
            format!(
                "tree blocks, flush, {j} of {} superblock copies",
                supers.len()
            ),
            patches,
        );
    }
    let mut torn = tree.clone();
    torn.push((supers[0].0, supers[0].1[..SUPER_LEN / 2].to_vec()));
    push(
        "tree blocks, flush, the primary superblock torn".into(),
        torn,
    );
    let mut reordered = tree.clone();
    reordered.extend(supers[1..].iter().cloned());
    push(
        "tree blocks, flush, the mirrors but not the primary (lost from the cache)".into(),
        reordered,
    );
    out
}

/// Cut one geometry's commit everywhere and judge every cut.
fn sweep(fixture_name: &str, label: &str) -> usize {
    let image = fixture(fixture_name);
    let ops = record_commit(&image, label);
    let cuts = cuts(&ops);

    // The patches cross the share as files; each distinct write is
    // written once and named by every cut that includes it.
    let dir = PathBuf::from(temp_path!("crash-{label}"));
    std::fs::create_dir_all(&dir).expect("creating the patch directory");
    let mut files: BTreeMap<(u64, Vec<u8>), PathBuf> = BTreeMap::new();
    let variants: Vec<Variant> = cuts
        .iter()
        .map(|cut| Variant {
            label: cut.label.clone(),
            patches: cut
                .patches
                .iter()
                .map(|(at, bytes)| {
                    let next = files.len();
                    let path = files
                        .entry((*at, bytes.clone()))
                        .or_insert_with(|| {
                            let path = dir.join(format!("patch-{next}.bin"));
                            std::fs::write(&path, bytes).expect("writing a patch");
                            path
                        })
                        .clone();
                    (*at, path)
                })
                .collect(),
        })
        .collect();

    let verdicts = guest_kernel_read_variants(&image, &variants, KERNEL_REPORT);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(verdicts.len(), cuts.len());

    // Before and after, as the kernel reads them — not as this crate
    // does, and not as the test assumes.
    let kernel_view = |v: &VariantVerdict| {
        assert!(
            v.mounted && v.report_status == 0,
            "[{label}] the kernel did not read `{}`:\n{}{}",
            v.label,
            v.mount_error,
            v.check_output
        );
        parse_report(&v.report)
    };
    let before = kernel_view(&verdicts[0]);
    let full = cuts
        .iter()
        .position(|c| c.primary_new && c.mirror_new)
        .expect("one cut is the whole commit");
    let after = kernel_view(&verdicts[full]);
    assert_eq!(
        before.get(CHANGED_FILE),
        Some(&sha256_hex(OLD_BYTES)),
        "[{label}] the untouched image does not hold {CHANGED_FILE} as the fixture wrote it"
    );
    assert_eq!(
        after.get(CHANGED_FILE),
        Some(&sha256_hex(NEW_BYTES)),
        "[{label}] the committed image does not show the transaction's change"
    );
    let mut rest_after = after.clone();
    rest_after.insert(CHANGED_FILE.into(), sha256_hex(OLD_BYTES));
    assert_eq!(
        rest_after, before,
        "[{label}] the commit changed something besides {CHANGED_FILE}"
    );

    let mut failures = Vec::new();
    for (cut, verdict) in cuts.iter().zip(&verdicts) {
        let mut fail = |what: String| failures.push(format!("{}: {what}", cut.label));

        // THE KERNEL reads the primary copy only. A torn primary is a
        // clean refusal; anything else must mount as exactly the state
        // that copy names.
        let kernel_expect = if cut.primary_torn {
            None
        } else if cut.primary_new {
            Some(State::After)
        } else {
            Some(State::Before)
        };
        let kernel_saw = if verdict.mounted {
            if verdict.report_status != 0 {
                fail(format!(
                    "the kernel mounted it and then could not read it (status {})",
                    verdict.report_status
                ));
            }
            match classify(&parse_report(&verdict.report), &before, &after) {
                Some(state) => Some(state),
                None => {
                    fail(format!(
                        "the kernel mounted it and showed NEITHER the old nor the new \
                         filesystem:\n{}",
                        verdict.report
                    ));
                    continue;
                }
            }
        } else {
            None
        };
        if kernel_saw != kernel_expect {
            fail(format!(
                "the kernel showed {kernel_saw:?} where the primary superblock names \
                 {kernel_expect:?}{}",
                if verdict.mounted {
                    String::new()
                } else {
                    format!(" (mount refused: {})", verdict.mount_error.trim())
                }
            ));
        }

        // THE CHECKER, whenever the kernel accepted the image. A verdict
        // that skipped something is not a verdict.
        if verdict.mounted {
            if !verdict.complaints.is_empty() {
                fail(format!(
                    "the kernel mounted it while complaining:\n    {}",
                    verdict.complaints.join("\n    ")
                ));
            }
            if verdict.check_status != 0 || !skipped(&verdict.check_output).is_empty() {
                fail(format!(
                    "btrfs check --readonly exited {} on an image the kernel mounted:\n{}",
                    verdict.check_status, verdict.check_output
                ));
            }
        }

        // THIS CRATE reads every copy and takes the newest that verifies
        // (#90), so a surviving mirror wins over a stale or torn primary.
        // It must still show exactly one side, and in that case it must
        // refuse to WRITE: a mount_rw on a non-primary copy is refused.
        let dev = Overlay::new(&image, cut.patches.clone());
        let crate_expect = if cut.primary_new || cut.mirror_new {
            State::After
        } else {
            State::Before
        };
        match Filesystem::mount(dev.clone() as Arc<dyn BlockRead>).and_then(|fs| crate_report(&fs))
        {
            Ok(seen) => match classify(&seen, &before, &after) {
                Some(state) if state == crate_expect => {}
                Some(state) => fail(format!(
                    "this crate showed {state:?}, not {crate_expect:?} — the newest \
                     superblock copy that verifies"
                )),
                None => fail(format!(
                    "this crate showed NEITHER the old nor the new filesystem: {seen:?}"
                )),
            },
            Err(error) => fail(format!("this crate refused it: {error}")),
        }
        let off_primary = cut.primary_torn || (cut.mirror_new && !cut.primary_new);
        if off_primary && Filesystem::mount_rw(dev as Arc<dyn BlockDevice>).is_ok() {
            fail(
                "this crate mounted it read-write from a superblock copy other than the \
                 primary"
                    .into(),
            );
        }
        if kernel_saw.is_some() && !off_primary && kernel_saw != Some(crate_expect) {
            fail(format!(
                "the kernel showed {kernel_saw:?} and this crate {crate_expect:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "[{label}] an interrupted commit left an image read wrongly:\n  {}",
        failures.join("\n  ")
    );
    println!(
        "[kernel vm] {label}: {} cuts of a {}-write commit, each old-or-new or refused",
        cuts.len(),
        ops.len()
    );
    cuts.len()
}

/// Cut a commit of this crate at every boundary and mount what is left.
///
/// Both geometries: the default single-copy metadata, and SHA-256 with
/// DUP metadata, where every tree block is two writes and a cut can fall
/// between a block's mirrors.
#[test]
fn an_interrupted_commit_mounts_as_before_or_after_and_never_between() {
    let mut judged = 0;
    for (name, label) in [
        ("btrfs-commit.img", "default"),
        ("btrfs-commit-sha256-dup.img", "sha256-dup"),
    ] {
        judged += sweep(name, label);
    }
    assert!(judged >= 16, "only {judged} cuts were judged");
}

/// What the fixture's recipe wrote (test-disks/guest-build-images.sh,
/// `build_dirtylog`): committed by a sync, then overwritten and fsynced
/// with a long commit interval, and a second file created and fsynced.
const DIRTY_LOG_FIXTURE: &str = "dirtylog/btrfs-dirty-log.img";
const COMMITTED: &[u8] = b"committed by a sync\n";
const FSYNCED: &[u8] = b"fsynced, not synced\n";
const CREATED: &[u8] = b"created then fsynced\n";

/// `log_root` as `btrfs inspect-internal dump-super` prints it.
fn dumped_log_root(image: &Path) -> u64 {
    let dump = dump_super(image);
    dump.lines()
        .find_map(|line| {
            let mut words = line.split_whitespace();
            (words.next() == Some("log_root"))
                .then(|| words.next().and_then(|n| n.parse().ok()))
                .flatten()
        })
        .unwrap_or_else(|| {
            panic!(
                "dump-super printed no log_root for {}:\n{dump}",
                image.display()
            )
        })
}

/// A volume with a log tree the kernel has not replayed is refused, by
/// every way in — never read around.
#[test]
fn an_unreplayed_log_tree_is_refused_rather_than_read_around() {
    let image = fixture(DIRTY_LOG_FIXTURE);
    assert_ne!(
        dumped_log_root(&image),
        0,
        "the fixture has no log tree, so this would test nothing: `chore fixtures` \
         builds it with one"
    );

    let refused = |what: &str, result: Result<Filesystem, Error>| match result {
        Err(Error::DirtyLog) => {}
        Err(error) => panic!("{what}: refused, but as {error:?} rather than DirtyLog"),
        Ok(fs) => panic!(
            "{what}: MOUNTED a volume with an unreplayed log tree; it reads {:?}",
            fs.read_path("/durable.txt")
                .map(|b| String::from_utf8_lossy(&b).into_owned())
        ),
    };
    let dev = Overlay::new(&image, Vec::new());
    refused(
        "mount",
        Filesystem::mount(dev.clone() as Arc<dyn BlockRead>),
    );
    refused(
        "mount_with_cache",
        Filesystem::mount_with_cache(dev.clone() as Arc<dyn BlockRead>, 64),
    );
    refused(
        "mount_rw",
        Filesystem::mount_rw(dev as Arc<dyn BlockDevice>),
    );
}

/// Why the refusal matters: the committed trees hold the file as it was
/// before the fsync, and the kernel's replay is what brings it forward.
///
/// With `log_root` cleared in every superblock copy — exactly what a
/// reader that ignored it would see — this crate reads the pre-fsync
/// bytes and no trace of the created file. The kernel, replaying the
/// log, reads what the application fsynced; and once it has, this crate
/// mounts the replayed image and reads the same.
#[test]
fn ignoring_the_log_would_return_pre_fsync_bytes_and_the_kernel_replays_it() {
    let image = fixture(DIRTY_LOG_FIXTURE);

    // What ignoring the log returns.
    let base = Overlay::new(&image, Vec::new());
    let mut cleared = Vec::new();
    for &copy in SUPER_COPIES.iter() {
        if copy + SUPER_LEN as u64 > base.size_bytes() {
            continue;
        }
        let mut raw = vec![0u8; SUPER_LEN];
        base.read_at(copy, &mut raw)
            .expect("reading a superblock copy");
        let csum_type = Superblock::parse_at(&raw, copy)
            .unwrap_or_else(|error| panic!("superblock copy at {copy}: {error}"))
            .csum_type;
        raw[LOG_ROOT..LOG_ROOT + 8].fill(0);
        raw[LOG_ROOT_LEVEL] = 0;
        fs_btrfs::super_write::stamp_checksum(&mut raw, csum_type);
        cleared.push((copy, raw));
    }
    let ignored = Filesystem::mount(Overlay::new(&image, cleared) as Arc<dyn BlockRead>)
        .expect("the committed trees, read with the log ignored");
    assert_eq!(
        ignored
            .read_path("/durable.txt")
            .expect("reading durable.txt"),
        COMMITTED,
        "the committed trees should hold durable.txt as it was before the fsync — \
         otherwise the fixture has nothing for the log to replay"
    );
    assert!(
        ignored.read_path("/fsynced.txt").is_err(),
        "the committed trees should not know the file created after the last commit"
    );

    // What the kernel's replay produces, on a copy.
    let copy = PathBuf::from(temp_path!("dirty-log-replayed.img"));
    std::fs::copy(&image, &copy).expect("copying the fixture to replay on");
    let out = guest_kernel_write_ok(
        &copy.to_string_lossy(),
        "log replay",
        "cat \"$MNT/durable.txt\" \"$MNT/fsynced.txt\"",
    );
    let expected = [FSYNCED, CREATED].concat();
    assert_eq!(
        out.as_bytes(),
        expected,
        "the kernel's replay did not produce what was fsynced"
    );
    assert_eq!(
        dumped_log_root(&copy),
        0,
        "the kernel mounted and unmounted the copy and left a log tree behind"
    );

    let replayed = Filesystem::mount(Overlay::new(&copy, Vec::new()) as Arc<dyn BlockRead>)
        .expect("this crate mounts the image the kernel replayed");
    assert_eq!(replayed.read_path("/durable.txt").unwrap(), FSYNCED);
    assert_eq!(replayed.read_path("/fsynced.txt").unwrap(), CREATED);
    let _ = std::fs::remove_file(&copy);
}
