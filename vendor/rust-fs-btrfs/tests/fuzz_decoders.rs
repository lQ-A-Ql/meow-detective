//! The stable-toolchain half of the fuzzing setup: replay the corpus,
//! then mutate it, and refuse if a decoder panics, hangs, or if the
//! suite quietly stopped doing any work.
//!
//! # Why there are two halves
//!
//! `fuzz/` holds `cargo-fuzz` targets. Those are the explorer: they run
//! for as long as you give them and find inputs nobody thought of. They
//! cannot be a required check, because how long they ran decides what
//! they found, and a fresh discovery would fail whichever unrelated
//! pull request happened to be open.
//!
//! This suite is the gate. Deterministic, on the stable toolchain, in
//! every pull request, reading the same `fuzz/corpus/` the explorer
//! does. Anything the explorer finds is committed there and replayed
//! here from then on.
//!
//! # Why there is no whole-image target
//!
//! Deliberate, not an oversight. The smallest filesystem `mkfs.btrfs`
//! will make is 16 MiB even with `--mixed`, which is over the 10 MiB
//! ceiling github-guard enforces on a committed file. Every structure
//! worth fuzzing here is reachable as a byte slice, and the mount path
//! is the best-covered thing in this repository already -- 47 of 49
//! suites reach the harness -- so the marginal value of carrying a
//! whole image is lower here than it was in the read-only drivers,
//! where kernel coverage was thin.
//!
//! # The checksum is re-stamped, and that is the point
//!
//! `TreeBlock::parse` verifies the checksum before it looks at
//! anything else. A mutated block is therefore rejected on the first
//! line, and the item walk -- the part with the arithmetic in it -- is
//! never reached. Left like that, this suite would spend its whole
//! budget proving that a checksum check works.
//!
//! A crafted image has a *valid* checksum: whoever wrote it computed
//! one, because they wanted the block to be read. So `restamp` is not a
//! cheat that weakens the test, it is what makes the test resemble the
//! threat.
//!
//! # The corpus
//!
//! Two filesystems `mkfs.btrfs` wrote, populated through `--rootdir`
//! rather than by mounting -- mounting needs root and `--rootdir` does
//! not. One `--mixed` at 16 MiB, which forces 4 KiB nodes, and one
//! ordinary at 256 MiB, which uses the 16 KiB default; the node size is
//! what every offset in a block is bounded by.
//!
//! Out of them come whole tree blocks at each level, and the item data
//! that leaves carry: inode items, directory items, xattr items and
//! chunk items. btrfs tree blocks have no magic number, so they are
//! found by the fsid every block repeats at offset 32.

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

/// Where the shared helpers look for the corpus, from this crate.
fn corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fuzz/corpus")
}

// The fixed superblock, the length normalisation and the checksum
// re-stamp, shared verbatim with the explorer. See
// fuzz/shared/helpers.rs for why they are included rather than
// depended on.
use std::sync::OnceLock;

include!("../fuzz/shared/helpers.rs");

/// Distinct starting points for the mutation stream. Fixed, so a
/// failure reproduces from the message alone.
const SEEDS: u64 = 6;

/// Below this, the suite is not doing its job.
const CASE_FLOOR: usize = 6_000;

/// Long enough that a loaded machine is never the reason, short enough
/// that a genuine hang is reported rather than left to the job timeout.
const DEADLINE: Duration = Duration::from_secs(180);

// ---------------------------------------------------------------- targets

struct Target {
    corpus: &'static str,
    name: &'static str,
    /// Mutated cases per (seed, starting point) pair.
    ///
    /// Per target rather than one constant, because a case costs what
    /// its seed costs to copy and to run. A 64 KiB region table is
    /// cheap; a 16 MB image opened and read is not, and giving both the
    /// same budget would mean either a slow gate or a shallow one.
    cases: usize,
    run: fn(&[u8]),
}

fn targets() -> Vec<Target> {
    vec![
        Target {
            corpus: "superblock",
            name: "superblock",
            cases: 256,
            run: |b| {
                let _ = fs_btrfs::Superblock::parse(b);
                let _ = fs_btrfs::superblock::Superblock::parse_at(b, 0x10000);
            },
        },
        Target {
            corpus: "tree_block",
            name: "tree_block",
            cases: 128,
            run: |b| {
                let geom = geometry();
                let mut node = node(b, geom.nodesize as usize);
                // Once as mutated -- which is what a torn write looks
                // like -- and once re-stamped, which is what a crafted
                // image looks like. The second is the one that reaches
                // the item walk.
                let _ = fs_btrfs::btree::Header::parse(&node);
                restamp(&mut node);
                let _ = fs_btrfs::btree::Header::parse(&node);
                let at = logical_of(&node);
                let _ = fs_btrfs::btree::TreeBlock::parse(node, at, geom);
            },
        },
        Target {
            corpus: "inode_item",
            name: "inode_item",
            cases: 256,
            run: |b| {
                let _ = fs_btrfs::inode::Inode::parse(b, 256);
            },
        },
        Target {
            corpus: "dir_items",
            name: "dir_items",
            cases: 256,
            run: |b| {
                let _ = fs_btrfs::dir::parse_dir_items(b);
            },
        },
        Target {
            corpus: "xattr_items",
            name: "xattr_items",
            cases: 256,
            run: |b| {
                let _ = fs_btrfs::xattr::parse_xattr_items(b);
            },
        },
        Target {
            corpus: "chunk_item",
            name: "chunk_item",
            cases: 256,
            run: |b| {
                let _ = fs_btrfs::Chunk::parse(0, b);
                let _ = fs_btrfs::chunk::Stripe::parse(b);
                let _ = fs_btrfs::chunk::DiskKey::parse(b);
            },
        },
    ]
}

// ---------------------------------------------------------------- corpus

fn seeds(corpus: &str) -> Vec<(String, Vec<u8>)> {
    let dir = corpus_root().join(corpus);
    let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("reading the corpus directory {}: {e}", dir.display()))
        .map(|entry| {
            let path = entry.expect("corpus directory entry").path();
            let bytes = std::fs::read(&path)
                .unwrap_or_else(|e| panic!("reading the seed {}: {e}", path.display()));
            let name = path
                .file_name()
                .expect("seed file name")
                .to_string_lossy()
                .into_owned();
            (name, bytes)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

// ---------------------------------------------------------------- mutation

/// xorshift64*. Small, deterministic, and not a dependency.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed ^ 0x9e37_79b9_7f4a_7c15)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            (self.next() % bound as u64) as usize
        }
    }
}

/// One mutation of a real structure or a real image, preserving length.
///
/// Length is preserved because a device answers a read past its end
/// with `ShortRead` before any of this code is reached -- a hostile
/// image controls what is in a block, not how many bytes the device
/// hands back.
///
/// The `header` bias exists because an image is mostly file data: a
/// uniformly random offset in a 48 KiB image lands in somebody's text
/// file nine times out of ten, where nothing parses it. Half the
/// mutations are aimed at the first two blocks, which is where the
/// superblock, the inode table and the directory blocks are.
fn mutate(seed: &[u8], rng: &mut Rng) -> Vec<u8> {
    let mut out = seed.to_vec();
    if out.is_empty() {
        return out;
    }
    let metadata_end = out.len().min(8192);
    let region = if rng.next() & 1 == 0 {
        metadata_end
    } else {
        out.len()
    };

    match rng.below(5) {
        0 => {
            for _ in 0..=rng.below(8) {
                let at = rng.below(region);
                out[at] ^= 1u8 << rng.below(8);
            }
        }
        1 => {
            let at = rng.below(region);
            let len = 1 + rng.below(16.min(out.len() - at));
            let fill = if rng.next() & 1 == 0 { 0x00 } else { 0xff };
            out[at..at + len].fill(fill);
        }
        2 => {
            let width = [2usize, 4, 8][rng.below(3)];
            if out.len() >= width {
                let at = rng.below(region.saturating_sub(width) + 1) & !(width - 1);
                if at + width <= out.len() {
                    let value: u64 = match rng.below(4) {
                        0 => 0,
                        1 => 1,
                        2 => u64::MAX,
                        _ => rng.next(),
                    };
                    // Little-endian: every multi-byte field in partition table is.
                    out[at..at + width].copy_from_slice(&value.to_le_bytes()[..width]);
                }
            }
        }
        3 => {
            if out.len() >= 8 {
                let a = rng.below(region / 4) * 4;
                let b = rng.below(region / 4) * 4;
                if a + 4 <= out.len() && b + 4 <= out.len() {
                    for i in 0..4 {
                        out.swap(a + i, b + i);
                    }
                }
            }
        }
        _ => {
            if out.len() >= 4 {
                let at = rng.below(region / 4) * 4;
                if at + 4 <= out.len() {
                    let word = u32::from_le_bytes(out[at..at + 4].try_into().expect("4 bytes"));
                    let delta = [1i64, -1, 2, -2, 255, -255][rng.below(6)];
                    let changed = (i64::from(word).wrapping_add(delta)) as u32;
                    out[at..at + 4].copy_from_slice(&changed.to_le_bytes());
                }
            }
        }
    }
    out
}

/// The case in flight, readable even if the lock was poisoned by the
/// panic we are trying to describe.
fn describe(current: &Arc<Mutex<String>>) -> String {
    match current.lock() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

// ---------------------------------------------------------------- tests

#[test]
fn every_target_has_a_corpus() {
    for target in targets() {
        assert!(
            !seeds(target.corpus).is_empty(),
            "the target {} reads fuzz/corpus/{}, which holds no seeds -- a target with an \
             empty corpus runs no cases and would pass in silence. Rebuild it with \
             scripts/make-fuzz-corpus.sh",
            target.name,
            target.corpus,
        );
    }
}

/// The corpus is an oracle, not just fuel: the committed superblocks are
/// ones `mkfs.btrfs` wrote, so this crate must read them and agree with
/// what the tool put there.
///
/// A seed that stopped parsing would otherwise go on being mutated and
/// go on not failing, because a mutation of an unreadable superblock is
/// also unreadable.
#[test]
fn every_committed_superblock_parses_and_describes_its_filesystem() {
    let found = seeds("superblock");
    assert_eq!(
        found.len(),
        2,
        "the superblock corpus holds {} seeds, not the two the script builds",
        found.len()
    );

    for (name, bytes) in found {
        let sb = fs_btrfs::Superblock::parse(&bytes).unwrap_or_else(|e| {
            panic!("{name}: a superblock mkfs.btrfs wrote would not parse: {e}")
        });

        // The node size is what every offset inside a tree block is
        // bounded by, so a wrong one is not a cosmetic disagreement.
        assert!(
            matches!(sb.nodesize, 4096 | 8192 | 16384 | 32768 | 65536),
            "{name}: nodesize {} is not one btrfs uses",
            sb.nodesize
        );
        // --mixed at 16 MiB forces 4 KiB nodes; the ordinary one takes
        // the 16 KiB default. If both came out the same, the script
        // stopped building two different filesystems and the corpus is
        // narrower than it looks.
        if name.starts_with("mixed") {
            assert_eq!(sb.nodesize, 4096, "{name}: --mixed should give 4 KiB nodes");
        } else {
            assert_eq!(
                sb.nodesize, 16384,
                "{name}: the default should be 16 KiB nodes"
            );
        }
    }
}

/// Every committed tree block must survive a re-stamp and parse.
///
/// This is what proves `restamp` works. If it computed the wrong
/// digest, every mutated block would be rejected at the checksum and
/// the tree_block target would be testing nothing -- and it would look
/// exactly like a target that was finding no bugs.
#[test]
fn a_restamped_tree_block_still_parses() {
    let geom = geometry();
    let found = seeds("tree_block");
    assert!(
        found.len() >= 6,
        "only {} tree blocks; the corpus has shrunk",
        found.len()
    );

    let mut parsed = 0;
    for (name, bytes) in found {
        if bytes.len() != geom.nodesize as usize {
            // A block from the other filesystem's node size; the
            // geometry here belongs to the 16 KiB one.
            continue;
        }
        let mut block = bytes.clone();
        restamp(&mut block);
        let at = logical_of(&block);
        fs_btrfs::btree::TreeBlock::parse(block, at, geom).unwrap_or_else(|e| {
            panic!(
                "{name}: a block mkfs.btrfs wrote would not parse after re-stamping: {e}\n\
                 If this says the checksum is wrong, `restamp` is computing the wrong digest \
                 and the tree_block target is exercising nothing."
            )
        });
        parsed += 1;
    }
    assert!(
        parsed > 0,
        "no committed tree block matched the geometry, so nothing was actually checked"
    );
}

#[test]
fn deterministic_mutations_of_real_structures_are_survived() {
    let cases = Arc::new(AtomicUsize::new(0));
    let current = Arc::new(Mutex::new(String::from("(not started)")));
    let (done_tx, done_rx) = mpsc::channel();

    let hook_current = Arc::clone(&current);
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        eprintln!("\nfuzz gate: panicked at {}", describe(&hook_current));
        previous_hook(info);
    }));

    let worker_cases = Arc::clone(&cases);
    let worker_current = Arc::clone(&current);
    let worker = std::thread::spawn(move || {
        for target in targets() {
            for (seed_name, bytes) in seeds(target.corpus) {
                for start in 0..SEEDS {
                    let mut rng = Rng::new(start);
                    for case in 0..target.cases {
                        *worker_current.lock().expect("progress lock") =
                            format!("{} / {seed_name} / seed {start} / case {case}", target.name);
                        let mutated = mutate(&bytes, &mut rng);
                        (target.run)(&mutated);
                        worker_cases.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }
        let _ = done_tx.send(());
    });

    // A timeout means the worker is still running: a hang. A disconnect
    // means it panicked, and the panic is what is worth reporting.
    match done_rx.recv_timeout(DEADLINE) {
        Ok(()) => {}
        Err(mpsc::RecvTimeoutError::Disconnected) => {}
        Err(mpsc::RecvTimeoutError::Timeout) => {
            // Written to the process's stderr rather than through
            // `eprintln!`, which the harness captures into a buffer it
            // only prints when a test finishes -- and exiting here means
            // it never finishes.
            let _ = writeln!(
                std::io::stderr(),
                "\nhung: no progress for {:?} at {}\n\
                 A decoder did not return. A tree whose node points back at itself \
                 looks exactly like this.",
                DEADLINE,
                describe(&current),
            );
            let _ = std::io::stderr().flush();
            std::process::exit(1);
        }
    }

    let outcome = worker.join();
    let _ = std::panic::take_hook();
    if outcome.is_err() {
        panic!("a decoder panicked at {}", describe(&current));
    }

    let total = cases.load(Ordering::Relaxed);
    assert!(
        total >= CASE_FLOOR,
        "only {total} mutated cases ran, below the floor of {CASE_FLOOR} -- the target \
         list or the corpus has collapsed, and a suite that runs nothing passes quickly",
    );
    eprintln!("{total} mutated cases");
}

#[test]
fn the_gate_covers_every_explorer_target() {
    // The two tiers drift apart the moment somebody adds a cargo-fuzz
    // target and forgets that nothing gates it on the stable toolchain.
    let manifest =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fuzz/Cargo.toml"))
            .expect("reading fuzz/Cargo.toml");

    let explorer: Vec<String> = manifest
        .lines()
        .filter_map(|line| line.strip_prefix("name = \""))
        .filter_map(|rest| rest.strip_suffix('"'))
        .map(str::to_owned)
        .skip(1) // the package name is the first `name =` in the file
        .collect();

    assert!(
        !explorer.is_empty(),
        "fuzz/Cargo.toml declares no [[bin]] targets",
    );

    let gated: Vec<&str> = targets().iter().map(|t| t.name).collect();
    for name in &explorer {
        assert!(
            gated.contains(&name.as_str()),
            "fuzz/fuzz_targets/{name}.rs has no counterpart in this suite, so nothing \
             replays its corpus on the stable toolchain and anything it finds would only \
             stay fixed for as long as somebody keeps running the fuzzer by hand",
        );
    }
}
