//! What a read costs, in calls to the device.
//!
//! # Why this is a test and not a benchmark
//!
//! The number that matters is not wall time. Wall time on a laptop with
//! a warm page cache says more about the laptop than the driver: run it
//! twice and the second is faster for reasons this repository does not
//! control. **Calls to the device** are deterministic — the same image
//! walked the same way makes the same calls every time — so they can be
//! asserted on, and a change that makes the driver ask for more is a
//! regression a test can catch rather than a number somebody has to
//! remember.
//!
//! Wall time is printed beside them, because it is what a user feels,
//! and ignored by the assertions.
//!
//! # What is measured
//!
//! Four shapes, because they cost differently and a change can improve
//! one while ruining another:
//!
//! - **mount** — opening the filesystem: the bootstrap, and nothing that
//!   grows with the volume. It loaded every item of the fs tree until
//!   #67, and a figure that grows with the size of the volume rather than
//!   with the work asked of it belongs to this line.
//! - **walk** — every directory in the tree, listed. Metadata only.
//! - **stat** — every file resolved by path from the root. Metadata,
//!   repeatedly, over the same nodes.
//! - **read** — every file's contents. Data, and the extent items that
//!   locate it.
//!
//! The node cache is why `stat` after `walk` reaches the device little or
//! not at all: the interior nodes every path descends were read by the
//! walk.
//!
//! The fixtures are gitignored and built by `chore fixtures`. A missing
//! one fails here rather than quietly measuring nothing: a cost test
//! that skipped is a cost test that reported no regression because it
//! read no image.

use fs_btrfs::Filesystem;
use fs_btrfs_test_support::fixture;
use fs_core::{CountingDevice, FileDevice};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

/// What one shape of read cost.
struct Cost {
    reads: u64,
    bytes: u64,
    micros: u128,
    /// How much work was actually done, so a number that fell because
    /// the driver did less is not read as a number that fell because
    /// the driver got better.
    items: usize,
}

/// What one pass measured, so the two can be compared rather than each
/// asserting on itself.
struct Pass {
    mount: Cost,
    walk: Cost,
    stat: Cost,
    read: Cost,
}

/// The three fixtures this measures, each on its own.
///
/// `docs/read-path-cost.md` publishes a table for `rich` and one for
/// `deep4k`, because they cost differently: a varied tree is a descent,
/// 20,000 files in one directory is one leaf scan repeated. This used to
/// return the first image found, from a list whose order contradicted
/// its own comment, so with the full matrix present only `deep4k` was
/// ever measured and the `rich` table could not be regenerated (#107).
///
/// All three are required. One that is absent takes a published table
/// with it, and a list that quietly shrank to what happened to be on
/// disk is how that went unnoticed the first time.
fn fixtures() -> Vec<PathBuf> {
    ["btrfs-rich.img", "btrfs-deep4k.img", "btrfs-commit.img"]
        .into_iter()
        .map(fixture)
        .collect()
}

/// The counter sits BELOW the cache, so what it reports is what
/// actually reached the device rather than what the driver asked for.
/// `blocks` of zero mounts without a cache, which is the baseline.
fn mount_counting(img: &Path, blocks: usize) -> (Filesystem, Arc<CountingDevice>, Cost) {
    let file = FileDevice::open(img).expect("open the fixture");
    let counting = Arc::new(CountingDevice::new(Arc::new(file)));
    let start = Instant::now();
    let fs = Filesystem::mount_with_cache(counting.clone(), blocks).expect("mount");
    let cost = Cost {
        reads: counting.reads(),
        bytes: counting.bytes(),
        micros: start.elapsed().as_micros(),
        items: 1,
    };
    (fs, counting, cost)
}

/// Every path in the tree, directories first, depth- and count-bounded
/// so a fixture with 20,000 files in one directory cannot make this run
/// for minutes.
fn walk_paths(fs: &Filesystem, at: &str, depth: u32, out: &mut Vec<(String, bool)>) {
    if depth == 0 || out.len() > 400 {
        return;
    }
    // A FAILURE IS A FAILURE, not a smaller tree (Greptile on #163). A
    // lookup or listing that failed used to drop the path or its subtree
    // silently, and the stat and read passes then did less work and still
    // passed. The depth and count bounds are the only reasons to stop.
    let dir = fs
        .lookup_path(at)
        .unwrap_or_else(|e| panic!("lookup_path({at}) failed during the walk: {e:?}"));
    let entries = fs
        .read_dir(dir.ino)
        .unwrap_or_else(|e| panic!("read_dir({at}) failed during the walk: {e:?}"));
    for e in entries {
        if out.len() > 400 {
            return;
        }
        let name = String::from_utf8_lossy(&e.name).to_string();
        let child = if at == "/" {
            format!("/{name}")
        } else {
            format!("{at}/{name}")
        };
        let inode = fs
            .lookup_path(&child)
            .unwrap_or_else(|e| panic!("lookup_path({child}) failed during the walk: {e:?}"));
        let is_dir = inode.is_dir();
        out.push((child.clone(), is_dir));
        if is_dir {
            walk_paths(fs, &child, depth - 1, out);
        }
    }
}

fn measure<F>(counting: &CountingDevice, items: usize, body: F) -> Cost
where
    F: FnOnce(),
{
    counting.reset();
    let start = Instant::now();
    body();
    Cost {
        reads: counting.reads(),
        bytes: counting.bytes(),
        micros: start.elapsed().as_micros(),
        items,
    }
}

fn report(what: &str, c: &Cost) {
    let per = if c.items == 0 {
        0.0
    } else {
        c.reads as f64 / c.items as f64
    };
    eprintln!(
        "{what:<6} {:>6} reads  {:>9} bytes  {:>8} µs  over {:>4} items  ({per:.1} reads/item)",
        c.reads, c.bytes, c.micros, c.items
    );
}

/// The measurement itself. Prints the numbers and asserts only that the
/// driver did the work and that the cache did not make it ask for more
/// — the figures are recorded in `docs/read-path-cost.md` and compared
/// by hand when something changes, because a threshold baked in here
/// would either be so loose it catches nothing or so tight it fails on
/// a fixture rebuild.
#[test]
fn what_a_read_costs_in_calls_to_the_device() {
    let images = fixtures();
    for img in &images {
        measure_fixture(img);
    }
}

fn measure_fixture(img: &Path) {
    eprintln!("=== measuring {}", img.display());

    eprintln!("--- uncached ---");
    let uncached = measure_one(img, 0);
    eprintln!("--- cached ---");
    let cached = measure_one(img, 512);

    // THE ASSERTIONS ARE ON THE UNCACHED PASS, because it is the one
    // that must reach the device: if the counter reports nothing there,
    // it is not wired to the mount and every figure above is fiction.
    // The cached pass is allowed to reach zero, so asserting the same
    // of it would be asserting that the cache failed. `stat` can reach
    // zero even uncached here, because the node cache holds the blocks
    // the walk read -- which is the finding rather than a broken counter.
    assert!(
        uncached.walk.items > 0,
        "the fixture had nothing to walk — the measurement is of nothing"
    );
    assert!(
        uncached.mount.reads > 0,
        "no calls reached the device, so the counter is not wired to the mount"
    );
    for (what, un, ca) in [
        ("mount", &uncached.mount, &cached.mount),
        ("walk", &uncached.walk, &cached.walk),
        ("stat", &uncached.stat, &cached.stat),
        ("read", &uncached.read, &cached.read),
    ] {
        // BYTES, NOT CALLS. A cache sized in sectors splits one node- or
        // extent-sized read into several, so a cached pass can legitimately
        // make MORE calls: that is the finding that decided
        // `DEFAULT_CACHE_BLOCKS`, not a regression. `mount` and `read` were
        // always exempt. Before #67, `walk` and `stat` made no calls at
        // all, because mount had loaded every item. Now they descend the
        // tree lazily, so they read nodes too, and they split the same way
        // (`btrfs-commit.img` in CI: a walk of 1 call uncached, 4 cached).
        // What a cache must never do is fetch more BYTES.
        assert!(
            ca.bytes <= un.bytes,
            "{what}: the cache made it fetch more bytes ({} vs {})",
            ca.bytes,
            un.bytes
        );
        assert_eq!(
            ca.items, un.items,
            "{what}: the two passes did different amounts of work, so the \
             figures are not comparable"
        );
    }
}

fn measure_one(img: &Path, blocks: usize) -> Pass {
    let (fs, counting, mount) = mount_counting(img, blocks);
    report("mount", &mount);

    let mut paths = Vec::new();
    let walk = measure(&counting, 0, || walk_paths(&fs, "/", 8, &mut paths));
    let walk = Cost {
        items: paths.len(),
        ..walk
    };
    report("walk", &walk);

    let files: Vec<String> = paths
        .iter()
        .filter(|(_, is_dir)| !*is_dir)
        .map(|(p, _)| p.clone())
        .collect();

    // WHAT THE DRIVER RETURNED, not what the device was asked for (#138).
    // The published `deep4k` row is zero reads and zero bytes for a
    // working driver, because the mount already loaded everything, so no
    // device-counter floor can tell a pass that did its work from one
    // that failed before any I/O. `items` for stat and read is therefore
    // the number of calls that SUCCEEDED, and a failure is a failure
    // rather than a shorter, cheaper pass.
    //
    // RESOLVING THE SAME PREFIXES AGAIN AND AGAIN is the shape a cache
    // is for: every path here descends from the root of the filesystem
    // tree through the same interior nodes.
    let mut resolved = 0usize;
    let mut declared = 0u64;
    let stat = measure(&counting, 0, || {
        for p in &files {
            match fs.lookup_path(p) {
                Ok(inode) => {
                    resolved += 1;
                    declared += inode.size;
                }
                Err(e) => panic!("lookup_path({p}) failed during the measurement: {e:?}"),
            }
        }
    });
    let stat = Cost {
        items: resolved,
        ..stat
    };
    report("stat", &stat);

    let mut read_ok = 0usize;
    let mut returned = 0u64;
    let read = measure(&counting, 0, || {
        for p in &files {
            match fs.read_path(p) {
                Ok(bytes) => {
                    read_ok += 1;
                    returned += bytes.len() as u64;
                }
                Err(e) => panic!("read_path({p}) failed during the measurement: {e:?}"),
            }
        }
    });
    let read = Cost {
        items: read_ok,
        ..read
    };
    report("read", &read);
    assert_eq!(
        (resolved, read_ok),
        (files.len(), files.len()),
        "not every walked file was resolved and read"
    );
    assert_eq!(
        returned, declared,
        "the reads returned {returned} bytes where the files declare {declared}"
    );
    assert!(
        files.is_empty() || returned > 0,
        "every read succeeded and returned no bytes, over {} files",
        files.len()
    );

    Pass {
        mount,
        walk,
        stat,
        read,
    }
}
