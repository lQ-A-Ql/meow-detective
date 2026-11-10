//! One bad copy of a tree block does not make a DUP volume unreadable.
//!
//! # What this holds that the unit tests do not
//!
//! `Tree::read_block`'s fallback is witnessed three ways in the unit
//! tests — the retry, which error is reported when no copy verifies, and
//! that a good first copy is not read twice. None of them reaches
//! `Filesystem`. Before this file existed, removing all four production
//! opt-ins together — that is, the driver never asking for a second
//! copy at all, which IS the filed defect — left the whole suite green
//! at **416 passed, 0 failed**, which is a figure from that tree and
//! is quoted as history rather than as a current measurement. With this
//! file present the same mutation is EXIT=101 with named failures here.
//!
//! THE OPT-INS ARE NAMED BY FUNCTION AS WELL AS BY LINE, because the
//! numbers in the first version of this paragraph were already stale by
//! the time it was reviewed and a stale pointer sends the next reader
//! to the wrong site:
//!
//! Each removed on its own, re-measured on THIS tree — the earlier
//! figures described a different one:
//!
//! | opt-in | at | removed alone |
//! |---|---|---|
//! | `open_pool`, chunk-tree bootstrap walk | `fs.rs:732` | EXIT=101, 4 named failures |
//! | `open_pool`, root-tree walk | `fs.rs:765` | EXIT=101, 2 named failures |
//! | `load_fs_tree` | `fs.rs:1007` | **green** — unwitnessed |
//! | `PoolReader::tree` | `fs.rs:1677` | **green** — unwitnessed |
//!
//! Two of the four are held, and the two that are not say so at their
//! own sites. Zero compile errors in every arm, counted separately from
//! named failures.
//!
//! So the mechanism was held and the plumbing was not. This file is the
//! plumbing: it damages a real `mkfs.btrfs` image and requires the
//! driver to mount it.
//!
//! # Why it is cheap, having been called expensive
//!
//! The first reading of this gap was that it needed a Linux-only image
//! builder of its own. It does not: `test-disks/btrfs-dup.img` is one of
//! the fixtures `chore fixtures` already builds, and everything below
//! works from it through the public API — no `mkfs.btrfs`, nothing this
//! file has to provision.
//!
//! # Vacuity
//!
//! This suite used to report `ok` having done nothing when the fixture
//! was absent — the shape of every oracle suite here, and the shape that
//! makes `13 passed in 0.00s` and `13 passed in 33.61s` look identical
//! in a log. THAT IS NOW IMPOSSIBLE RATHER THAN MERELY DOCUMENTED: every
//! image below comes from `fs_btrfs_test_support::fixture`, which fails
//! the test and names the task that builds the image instead of handing
//! back a `None` for the test to skip on.
//!
//! The other half of vacuity is still this file's own to hold, because
//! no helper can: it prints what it did and how long the image was, and
//! the assertions below check the fixture is a DUP one before believing
//! any pass — a chunk with one copy has nothing to fall back to, and a
//! test that "passed" on it would mean nothing at all.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fs_btrfs::superblock::SUPER_INFO_OFFSET;
use fs_btrfs::{ChunkMap, Superblock};
use fs_btrfs_test_support::{fixture, temp_path, Image};
use fs_core::FileDevice;

fn dup_image() -> PathBuf {
    fixture("btrfs-dup.img")
}

/// A copy of a fixture, somewhere unique to this process, with a guard
/// that removes it so a panic mid-test does not leave the image behind.
///
/// COPIED BY THE FILESYSTEM, and damaged a byte at a time. It used to
/// read the fixture into a `Vec`, clone it per case and write the clone
/// out: `btrfs-pool-a.img` is 512 MiB, seven tests run at once, and the
/// harness guest has four gigabytes — so the guest's kernel killed this
/// binary outright. Nothing here ever wanted the whole image; each case
/// flips one byte.
struct Scratch(PathBuf);

impl Scratch {
    fn copy_of(tag: &str, src: &Path) -> Self {
        // In the repository's own scratch directory, not the system
        // one: these images are what the oracle tools would be pointed
        // at, and the harness VM sees this repository and nothing else
        // of the host.
        let path = PathBuf::from(temp_path!(
            "btrfs-dup-{tag}-{}-{:?}.img",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::copy(src, &path)
            .unwrap_or_else(|e| panic!("copying {} to {}: {e}", src.display(), path.display()));
        Self(path)
    }

    /// Flip every bit of the byte at `offset`, in the copy.
    fn flip(&self, offset: u64) {
        use std::os::unix::fs::FileExt;
        let file = std::fs::File::options()
            .read(true)
            .write(true)
            .open(&self.0)
            .unwrap_or_else(|e| panic!("opening {} to damage it: {e}", self.0.display()));
        let mut byte = [0u8; 1];
        file.read_exact_at(&mut byte, offset)
            .unwrap_or_else(|e| panic!("reading {} at {offset}: {e}", self.0.display()));
        byte[0] ^= 0xFF;
        file.write_all_at(&byte, offset)
            .unwrap_or_else(|e| panic!("damaging {} at {offset}: {e}", self.0.display()));
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn a_damaged_first_copy_of_the_chunk_root_does_not_stop_the_mount() {
    let src = dup_image();
    let source = Image::open(&src);
    eprintln!(
        "dup_mirror_fallback: {} ({} bytes)",
        src.display(),
        source.len()
    );

    let head = source.read_at(SUPER_INFO_OFFSET, 4096);
    let sb = Superblock::parse_at(&head, SUPER_INFO_OFFSET).expect("superblock");
    let map = ChunkMap::bootstrap(&sb).expect("bootstrap");

    // THE HARNESS CHECKS BEFORE THE ASSERTION. A chunk with one copy has
    // nothing to fall back to, so a pass below would say nothing about
    // the fallback — it would say the damage missed.
    let addr = sb.chunk_root;
    let copies = map.mirrors_at(addr).expect("mirrors_at");
    assert!(
        copies >= 2,
        "chunk_root has {copies} copy; btrfs-dup.img is not a DUP fixture and this arm \
         cannot mean anything on it"
    );
    let first = map.map_mirror(addr, 0).expect("mirror 0");
    let second = map.map_mirror(addr, 1).expect("mirror 1");
    assert_ne!(
        first.physical, second.physical,
        "the two copies share a physical offset, so damaging one damages both"
    );

    // The control: the image mounts before anything is done to it. A
    // fixture that could not mount would make the interesting result
    // below indistinguishable from a broken image.
    {
        let pristine = Scratch::copy_of("pristine", &src);
        let dev = FileDevice::open(&pristine.0).expect("open pristine");
        fs_btrfs::fs::Filesystem::mount(Arc::new(dev)).expect("the undamaged fixture must mount");
    }

    // ONE BYTE IN THE ITEM AREA OF COPY 0, past the 101-byte header — so
    // the header still claims to be this block and the checksum is the
    // only thing that catches it. That is what a bad sector produces,
    // and it is the case the fallback exists for; damaging the header
    // instead would be caught by the identity fields and prove less.
    let image = Scratch::copy_of("damaged", &src);
    image.flip(first.physical + 200);
    let dev = FileDevice::open(&image.0).expect("open damaged");
    fs_btrfs::fs::Filesystem::mount(Arc::new(dev)).unwrap_or_else(|e| {
        panic!(
            "copy 0 of chunk_root at physical {} was damaged and copy 1 at {} is intact; \
             the mount must fall back to it. Got {e:?}",
            first.physical, second.physical
        )
    });
    eprintln!("dup_mirror_fallback: mounted with copy 0 of chunk_root damaged");
}

/// THE CHUNK ROOT IS ONE OF FOUR OPT-INS, AND IT ONLY REACHES ONE.
///
/// `Filesystem` asks for redundancy in four places: the chunk-tree
/// bootstrap walk, the root-tree walk, the fs-tree walk and the pool's
/// tree reader. Damaging `chunk_root` reaches the first alone —
/// measured: removing each of the four individually, only the first
/// fails the test above.
///
/// So this damages copy 0 of every tree root the superblock names that
/// has a second copy, one image per root, and requires the mount each
/// time. It asserts how many roots it actually exercised, because a
/// version that silently found one would look exactly like this one
/// passing.
#[test]
fn a_damaged_first_copy_of_any_named_root_does_not_stop_the_mount() {
    let src = dup_image();
    let head = Image::open(&src).read_at(SUPER_INFO_OFFSET, 4096);
    let sb = Superblock::parse_at(&head, SUPER_INFO_OFFSET).expect("superblock");

    // THE FULL MAP, NOT THE BOOTSTRAP ONE. `ChunkMap::bootstrap` knows
    // only the system chunks embedded in the superblock, so it cannot
    // resolve `sb.root` at all — and a version of this test built on it
    // silently exercised one root while claiming two. Mounting the
    // pristine image builds the real map, which is also the thing under
    // test having worked.
    let pristine = Scratch::copy_of("map", &src);
    let dev = FileDevice::open(&pristine.0).expect("open pristine");
    let mounted = fs_btrfs::fs::Filesystem::mount(Arc::new(dev)).expect("pristine must mount");
    let map = mounted.chunk_map();

    let mut exercised = 0usize;
    for (what, addr) in [("chunk_root", sb.chunk_root), ("root", sb.root)] {
        let Ok(copies) = map.mirrors_at(addr) else {
            continue;
        };
        if copies < 2 {
            eprintln!("dup_mirror_fallback: {what} has {copies} copy; nothing to fall back to");
            continue;
        }
        let first = map.map_mirror(addr, 0).expect("mirror 0");
        let second = map.map_mirror(addr, 1).expect("mirror 1");
        assert_ne!(first.physical, second.physical, "{what}: copies coincide");

        let image = Scratch::copy_of(what, &src);
        image.flip(first.physical + 200);
        let dev = FileDevice::open(&image.0).expect("open damaged");
        fs_btrfs::fs::Filesystem::mount(Arc::new(dev)).unwrap_or_else(|e| {
            panic!(
                "{what}: copy 0 at physical {} damaged, copy 1 at {} intact; \
                 the mount must fall back to it. Got {e:?}",
                first.physical, second.physical
            )
        });
        eprintln!("dup_mirror_fallback: mounted with copy 0 of {what} damaged");
        exercised += 1;
    }

    assert!(
        exercised >= 2,
        "only {exercised} root(s) had a second copy on this fixture, so this test \
         reached fewer opt-ins than it claims to"
    );
}

/// A POOL'S SECOND COPY IS ON THE OTHER DISK, and following it means
/// following the devid the mapping names, not just its offset.
///
/// THIS DOES NOT REACH `PoolReader::tree` (`fs.rs:1677`, inside
/// `impl PoolReader<'_>`) — measured, and RE-measured on this tree
/// rather than renumbered: removing that opt-in alone still leaves this
/// suite green, and the site says so. The number was stale and the
/// sentence is a measured negative result rather than a pointer, so
/// correcting the digits without re-running the mutation would have
/// turned a fact into an assertion nobody had checked. What it holds is the pool form of the two opt-ins a mount
/// does reach, whose mirror read must be `read_logical_pool_mirror`
/// rather than the single-device form: reverting either one alone fails
/// this test and nothing else. It is served by a different fixture:
/// `test-disks/btrfs-pool-{a,b}.img`, a real two-device filesystem whose
/// metadata mkfs put in RAID1 — so a tree block's two copies are on
/// DIFFERENT devices, and damaging one means writing to one image and
/// leaving the other alone.
///
/// A NOTE SAYING "THE POOL CASE IS UNREACHABLE" WAS ABOUT TO SHIP.
/// The fixture already ships, the way `btrfs-dup.img` did, and writing
/// the test instead found the fallback did not work on a two-device
/// pool at all — the RAID1 half of the case the issue was filed for.
/// Establish the wall by trying.
#[test]
fn a_damaged_copy_on_one_pool_device_does_not_stop_the_pool_reading() {
    // Both halves, or neither: one member of a two-device filesystem is
    // not a pool, and a missing one fails here naming `chore fixtures`.
    let (a, b) = (fixture("btrfs-pool-a.img"), fixture("btrfs-pool-b.img"));

    let open = |pa: &Path, pb: &Path| -> Vec<Arc<dyn fs_core::BlockRead>> {
        vec![
            Arc::new(FileDevice::open(pa).expect("open a")) as Arc<dyn fs_core::BlockRead>,
            Arc::new(FileDevice::open(pb).expect("open b")) as Arc<dyn fs_core::BlockRead>,
        ]
    };

    // The control, and the map: a pristine pool opens, and the mount is
    // what builds the ChunkMap that says where the copies live.
    let pristine_a = Scratch::copy_of("pool-a", &a);
    let pristine_b = Scratch::copy_of("pool-b", &b);
    let fs = fs_btrfs::fs::Filesystem::mount_pool(open(&pristine_a.0, &pristine_b.0))
        .expect("the undamaged pool must open");
    fs.list_path("/").expect("the undamaged pool must read");
    let sb = fs.superblock().clone();
    let map = fs.chunk_map();

    // BOTH ROOTS, one damaged image pair each. `sb.root` alone reaches
    // the root-tree walk; `sb.chunk_root` is what the bootstrap walk
    // reads, and on this fixture its two copies really are on different
    // devices -- printed below, because if they were not, the pool-aware
    // read there could not be witnessed here at all.
    let mut exercised = 0usize;
    for (what, addr) in [("chunk_root", sb.chunk_root), ("root", sb.root)] {
        let Ok(copies) = map.mirrors_at(addr) else {
            continue;
        };
        if copies < 2 {
            eprintln!(
                "dup_mirror_fallback: pool {what} has {copies} copy; nothing to fall back to"
            );
            continue;
        }
        let first = map.map_mirror(addr, 0).expect("mirror 0");
        let second = map.map_mirror(addr, 1).expect("mirror 1");
        assert_ne!(
            (first.devid, first.physical),
            (second.devid, second.physical),
            "{what}: the two copies are the same bytes on the same device"
        );
        eprintln!(
            "dup_mirror_fallback: pool {what} copies on devid {} @ {} and devid {} @ {}",
            first.devid, first.physical, second.devid, second.physical
        );

        // devid 1 is image a, devid 2 is image b -- asserted rather than
        // assumed, because getting it the wrong way round damages the
        // copy this test needs intact and the failure would look like
        // the fallback not working.
        assert!(
            first.devid == 1 || first.devid == 2,
            "{what}: unexpected devid {}",
            first.devid
        );
        let da = Scratch::copy_of("pool-a-damaged", &a);
        let db = Scratch::copy_of("pool-b-damaged", &b);
        if first.devid == 1 {
            da.flip(first.physical + 200);
        } else {
            db.flip(first.physical + 200);
        }
        let fs = fs_btrfs::fs::Filesystem::mount_pool(open(&da.0, &db.0)).unwrap_or_else(|e| {
            panic!(
                "{what}: copy 0 at devid {} physical {} was damaged and copy 1 at devid {} \
                 physical {} is intact; the pool must fall back to it. Got {e:?}",
                first.devid, first.physical, second.devid, second.physical
            )
        });
        fs.list_path("/")
            .unwrap_or_else(|e| panic!("{what}: and the pool must still read: {e}"));
        eprintln!("dup_mirror_fallback: pool read with copy 0 of {what} damaged");
        exercised += 1;
    }

    assert!(
        exercised >= 2,
        "only {exercised} pool root(s) had a second copy, so this test reached fewer \
         opt-ins than it claims to"
    );
}

/// THE ACCEPTANCE HALF, AND IT VARIES WHICH COPY IS ROTTEN RATHER THAN
/// WHETHER ONE IS.
///
/// Every test above damages copy 0. A fix that read every mirror and
/// required them all to verify would pass all of them and would refuse
/// this: a volume whose FIRST copy is good and whose second has rotted
/// is a volume the kernel mounts without a word, and it is the common
/// case on a disk with one bad sector, since which copy the sector
/// lands under is a coin toss.
///
/// It is deliberately not witnessed by any mutation of the fallback —
/// removing the fallback entirely leaves it green, because copy 0 is
/// intact. That is what an acceptance case is for. What it fails is the
/// over-correction, which no defeat case here can see.
#[test]
fn a_damaged_second_copy_is_not_noticed_at_all() {
    let src = dup_image();
    let head = Image::open(&src).read_at(SUPER_INFO_OFFSET, 4096);
    let sb = Superblock::parse_at(&head, SUPER_INFO_OFFSET).expect("superblock");

    let pristine = Scratch::copy_of("map2", &src);
    let dev = FileDevice::open(&pristine.0).expect("open pristine");
    let mounted = fs_btrfs::fs::Filesystem::mount(Arc::new(dev)).expect("pristine must mount");
    let map = mounted.chunk_map();

    let mut exercised = 0usize;
    for (what, addr) in [("chunk_root", sb.chunk_root), ("root", sb.root)] {
        let Ok(copies) = map.mirrors_at(addr) else {
            continue;
        };
        if copies < 2 {
            continue;
        }
        let first = map.map_mirror(addr, 0).expect("mirror 0");
        let second = map.map_mirror(addr, 1).expect("mirror 1");
        assert_ne!(first.physical, second.physical, "{what}: copies coincide");

        let image = Scratch::copy_of(&format!("{what}-second"), &src);
        image.flip(second.physical + 200);
        let dev = FileDevice::open(&image.0).expect("open damaged");
        let fs = fs_btrfs::fs::Filesystem::mount(Arc::new(dev)).unwrap_or_else(|e| {
            panic!(
                "{what}: copy 1 at physical {} is damaged and copy 0 at {} is intact; \
                 a healthy first copy must be enough. Got {e:?}",
                second.physical, first.physical
            )
        });
        fs.list_path("/")
            .unwrap_or_else(|e| panic!("{what}: and it must still read: {e}"));
        eprintln!("dup_mirror_fallback: mounted with copy 1 of {what} damaged");
        exercised += 1;
    }

    assert!(
        exercised >= 2,
        "only {exercised} root(s) had a second copy, so this acceptance case covered \
         less than it claims to"
    );
}

/// The other direction, and it is the one that stops the tests above
/// passing for the wrong reason: damage BOTH copies and the mount must
/// refuse. Without this, a driver that ignored checksums entirely would
/// satisfy the fallback assertion.
#[test]
fn damaging_every_copy_is_still_refused() {
    let src = dup_image();
    let head = Image::open(&src).read_at(SUPER_INFO_OFFSET, 4096);
    let sb = Superblock::parse_at(&head, SUPER_INFO_OFFSET).expect("superblock");
    let map = ChunkMap::bootstrap(&sb).expect("bootstrap");

    let addr = sb.chunk_root;
    let copies = map.mirrors_at(addr).expect("mirrors_at");
    assert!(copies >= 2, "not a DUP fixture");

    let image = Scratch::copy_of("both", &src);
    for mirror in 0..copies {
        let m = map.map_mirror(addr, mirror).expect("mirror");
        image.flip(m.physical + 200);
    }

    let dev = FileDevice::open(&image.0).expect("open damaged");
    let err = fs_btrfs::fs::Filesystem::mount(Arc::new(dev))
        .err()
        .expect("with every copy damaged the mount must refuse, not fall back to nothing");
    eprintln!("dup_mirror_fallback: both copies damaged -> {err:?}");
}

/// AN UNREADABLE COPY IS THE CASE THIS ISSUE IS NAMED FOR, AND IT WAS
/// THE ONE STILL BROKEN.
///
/// Every other test here damages a BYTE: the read succeeds and
/// `TreeBlock::parse` refuses the checksum. That is not how a disk
/// usually goes bad. A bad sector makes the read itself fail, and the
/// primary read was `(self.read)(logical, &mut buf)?` — a `?` that
/// returned before `redundancy` was consulted at all. So the retry ran
/// only when copy 0 could be read and merely failed to verify, while
/// inside the mirror loop the same I/O error had always been a
/// `continue`. The two halves of one fallback disagreed about whether
/// an unreadable copy is a reason to try the other one.
///
/// This is not reachable by damaging the image, which is why the eight
/// mutation arms on this branch could not see it: it needs a device
/// that returns `Err`, not a device that returns wrong bytes.
/// [`Unreadable`] is that device, and it fails exactly the sectors copy
/// 0 occupies and nothing else — asserted below by reading copy 1
/// through the same wrapper first, so a pass cannot come from the
/// wrapper failing everything or nothing.
struct Unreadable {
    inner: FileDevice,
    bad: std::ops::Range<u64>,
    /// Reads refused, so the test can prove the wrapper actually fired
    /// rather than having been bypassed.
    refused: std::sync::atomic::AtomicUsize,
}

impl fs_core::BlockRead for Unreadable {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> fs_core::Result<()> {
        let end = offset + buf.len() as u64;
        if offset < self.bad.end && end > self.bad.start {
            self.refused
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Err(fs_core::Error::Io(std::io::Error::other(
                "simulated bad sector",
            )));
        }
        self.inner.read_at(offset, buf)
    }

    fn size_bytes(&self) -> u64 {
        self.inner.size_bytes()
    }
}

#[test]
fn an_unreadable_first_copy_does_not_stop_the_mount() {
    let src = dup_image();
    let head = Image::open(&src).read_at(SUPER_INFO_OFFSET, 4096);
    let sb = Superblock::parse_at(&head, SUPER_INFO_OFFSET).expect("superblock");
    let map = ChunkMap::bootstrap(&sb).expect("bootstrap");

    let addr = sb.chunk_root;
    let copies = map.mirrors_at(addr).expect("mirrors_at");
    assert!(
        copies >= 2,
        "not a DUP fixture: {addr:#x} has {copies} copy, so there is nothing to fall back to"
    );
    let first = map.map_mirror(addr, 0).expect("mirror 0");
    let second = map.map_mirror(addr, 1).expect("mirror 1");
    assert_ne!(
        first.physical, second.physical,
        "the two copies are the same bytes"
    );

    let pristine = Scratch::copy_of("unreadable", &src);
    let nodesize = sb.nodesize as u64;
    let bad = first.physical..first.physical + nodesize;

    // THE WRAPPER IS CHECKED BEFORE IT IS TRUSTED. Copy 1 must still
    // read through it, or "the mount succeeded" would only mean the
    // damage missed; and copy 0 must actually be refused, or it would
    // only mean the wrapper missed.
    {
        let probe = Unreadable {
            inner: FileDevice::open(&pristine.0).expect("open"),
            bad: bad.clone(),
            refused: std::sync::atomic::AtomicUsize::new(0),
        };
        let mut buf = vec![0u8; nodesize as usize];
        fs_core::BlockRead::read_at(&probe, second.physical, &mut buf)
            .expect("copy 1 must still be readable through the wrapper");
        fs_core::BlockRead::read_at(&probe, first.physical, &mut buf)
            .expect_err("copy 0 must be refused by the wrapper");
        assert_eq!(
            probe.refused.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "the wrapper must refuse copy 0 exactly once, not everything and not nothing"
        );
    }

    let dev = Arc::new(Unreadable {
        inner: FileDevice::open(&pristine.0).expect("open"),
        bad,
        refused: std::sync::atomic::AtomicUsize::new(0),
    });
    let fs = fs_btrfs::fs::Filesystem::mount(dev.clone()).unwrap_or_else(|e| {
        panic!(
            "copy 0 of chunk_root at physical {} is UNREADABLE and copy 1 at {} is intact; \
             the mount must fall back to it rather than propagating the I/O error. Got {e:?}",
            first.physical, second.physical
        )
    });
    fs.list_path("/")
        .expect("and it must still read the root directory");
    let refused = dev.refused.load(std::sync::atomic::Ordering::Relaxed);
    assert!(
        refused >= 1,
        "the mount never tried to read copy 0, so this test did not exercise the fallback"
    );
    eprintln!("dup_mirror_fallback: mounted with copy 0 of chunk_root unreadable ({refused} refused reads)");
}

/// AND WHEN NOTHING IS LEFT, IT IS THE I/O ERROR THAT IS REPORTED.
///
/// `read_block` promises "the error returned is copy 0's". Making the
/// primary read fall through rather than `?` opens a way to break that
/// promise while still passing every fallback test: parse the buffer
/// anyway. A zeroed or half-written buffer fails its checksum, so the
/// mount would report `ChecksumMismatch` for a disk that returned an
/// I/O error — describing the wreckage instead of the damage, and
/// sending whoever reads the message looking for corruption rather than
/// for a bad sector.
///
/// So: copy 0 unreadable, copy 1 damaged, nothing to fall back to, and
/// the error must still be the read's.
#[test]
fn an_unreadable_first_copy_reports_the_io_error_not_a_checksum() {
    let src = dup_image();
    let head = Image::open(&src).read_at(SUPER_INFO_OFFSET, 4096);
    let sb = Superblock::parse_at(&head, SUPER_INFO_OFFSET).expect("superblock");
    let map = ChunkMap::bootstrap(&sb).expect("bootstrap");

    let addr = sb.chunk_root;
    assert!(
        map.mirrors_at(addr).expect("mirrors_at") >= 2,
        "not a DUP fixture"
    );
    let first = map.map_mirror(addr, 0).expect("mirror 0");
    let second = map.map_mirror(addr, 1).expect("mirror 1");

    // Copy 1 rotted, copy 0 unreadable.
    let image = Scratch::copy_of("unreadable-both", &src);
    image.flip(second.physical + 200);

    let dev = Arc::new(Unreadable {
        inner: FileDevice::open(&image.0).expect("open"),
        bad: first.physical..first.physical + sb.nodesize as u64,
        refused: std::sync::atomic::AtomicUsize::new(0),
    });
    let err = fs_btrfs::fs::Filesystem::mount(dev)
        .err()
        .expect("with copy 0 unreadable and copy 1 rotten the mount must refuse");
    assert!(
        matches!(err, fs_btrfs::Error::Io(_)),
        "copy 0 returned an I/O error, so that is what must be reported rather than a \
         checksum verdict on a buffer that was never filled. Got {err:?}"
    );
    eprintln!("dup_mirror_fallback: copy 0 unreadable + copy 1 rotten -> {err:?}");
}
