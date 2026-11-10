//! THE KERNEL DECIDES. The in-kernel btrfs driver mounts every fixture,
//! writes to it and reads it back; and the whole write path is performed
//! by this crate and then handed to `btrfs check` and to a real mount.
//!
//! # Why this file exists
//!
//! Everything else in this suite compares against bytes the kernel
//! already wrote, or against what btrfs-progs says about them. That is a
//! second opinion on the same file format from the same project, and it
//! cannot catch a volume it calls valid that Linux nevertheless mounts
//! differently. This asks the kernel to accept bytes WE wrote — the only
//! judgement that is not a form of agreeing with ourselves.
//!
//! # Where it runs, and why that changed
//!
//! In the fs-linux-test-harness guest, through
//! `fs_btrfs_test_support::guest_kernel_*`. These checks used to live in
//! `.github/workflows/ci.yml` as three steps that ran `sudo mount -o
//! loop` ON THE RUNNER: a `for img in .vm-share/btrfs-*.img` mount loop,
//! a `cargo run --example write_transaction` followed by `sudo btrfs
//! check` and a mount, and a `dmesg` scrape. They worked, and they were
//! unavailable anywhere else — a macOS developer could not run the
//! kernel half of this repository's gate at all, and a Linux developer
//! could only run it by handing a test suite root. Moving them into the
//! guest makes them the same checks in CI and on a workstation, and
//! makes them ordinary tests: they are selected by tier
//! (`scripts/test-targets.sh` puts anything calling `guest_kernel_*` in
//! `chore test:kernel`) rather than by a step somebody remembered to add.
//!
//! # A mount is not free of consequences
//!
//! Mounting btrfs read-write bumps the generation and rewrites the
//! superblock whatever else happens, and every fixture here is compared
//! elsewhere against a `dump-super` report taken straight out of
//! `mkfs.btrfs`. So the round trip happens on a copy the guest throws
//! away (`guest_kernel_probe`), and only the images this test writes for
//! itself come back.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fs_btrfs::fs::Filesystem;
use fs_btrfs::super_write::Commit;
use fs_btrfs_test_support::{
    assert_btrfs_check_clean, fixture, fixtures_matching, guest_kernel_probe,
    guest_kernel_write_ok, spans_several_devices, temp_path,
};
use fs_core::{BlockDevice, FileDevice};

/// What the probe writes and expects back. Long enough to be more than
/// one inline byte and short enough to read in a failure message.
const PAYLOAD: &str = "kernel round-trip";

/// Every fixture the kernel is expected to mount on its own.
///
/// A member of a multi-device filesystem is excluded, and that is not a
/// skip: the KERNEL refuses it for the same reason this driver does,
/// since half the chunks live on the other disk. Mounting a pool needs
/// both devices registered, which is a different test from this one —
/// `tests/pool_oracle.rs` is where such an image is the subject.
fn single_device_fixtures() -> Vec<PathBuf> {
    let all = fixtures_matching("btrfs-");
    let (single, pooled): (Vec<PathBuf>, Vec<PathBuf>) = all
        .into_iter()
        .partition(|image| !spans_several_devices(image));
    for image in &pooled {
        println!(
            "[kernel vm] set aside {}: it is one device of a multi-device filesystem",
            name(image)
        );
    }
    assert!(
        !single.is_empty(),
        "every fixture spans several devices, so this would mount nothing"
    );
    single
}

fn name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// Proof that the kernel itself accepts these images: mount each one
/// with the in-kernel btrfs driver, write a file, read it back, unmount
/// cleanly — and log nothing about it.
///
/// A mount that succeeds but cannot do IO is not a pass. Neither is one
/// the kernel completes while complaining: the kernel is chatty when it
/// is unhappy and silent when it is not, so anything above info level
/// about btrfs during one of these mounts means the image is malformed
/// in a way the mount itself tolerated — exactly the class of defect a
/// self-consistent parser produces.
#[test]
fn the_kernel_mounts_every_fixture_and_reads_back_what_it_wrote() {
    let mut mounted = 0;
    let mut refused = Vec::new();
    let mut noisy = Vec::new();

    for image in single_device_fixtures() {
        let probe = guest_kernel_probe(&image.to_string_lossy(), PAYLOAD);
        if probe.readback != PAYLOAD {
            refused.push(format!(
                "{}: read back {:?}, not {PAYLOAD:?}",
                name(&image),
                probe.readback
            ));
        }
        if !probe.complaints.is_empty() {
            noisy.push(format!(
                "{}:\n    {}",
                name(&image),
                probe.complaints.join("\n    ")
            ));
        }
        mounted += 1;
    }

    assert!(
        refused.is_empty(),
        "the in-kernel btrfs driver did not round-trip these images:\n  {}",
        refused.join("\n  ")
    );
    assert!(
        noisy.is_empty(),
        "the kernel mounted these images while complaining about them, which is not a \
         pass — a mount Linux tolerates is not the same as an image Linux agrees with:\n  {}",
        noisy.join("\n  ")
    );
    assert!(mounted > 0, "no image was mounted, so this checked nothing");
    println!("[kernel vm] {mounted} fixtures mounted, written and read back cleanly");
}

/// One whole transaction, performed by this crate, then judged by the
/// reference checker AND by a real mount.
///
/// This is the gate that cannot be satisfied by agreeing with ourselves.
/// Everything else compares against bytes the kernel already wrote; this
/// asks it to accept bytes we wrote — and then asks the checker again
/// afterwards, because a filesystem the kernel mounted and used can be
/// left in a state the checker rejects even when the mount went fine.
///
/// Both geometries, in one run: the default crc32c volume and the
/// SHA-256 DUP one, whose checksum width and chunk layout are what catch
/// a writer that hardcoded either.
#[test]
fn a_transaction_this_driver_wrote_is_accepted_by_the_checker_and_the_kernel() {
    let mut done = 0;
    for (label, suffix) in [("default geometry", ""), ("sha256+dup", "-sha256-dup")] {
        let source = fixture(&format!("btrfs-cow-before{suffix}.img"));
        let written = PathBuf::from(temp_path!("written{}.img", suffix));
        std::fs::copy(&source, &written).expect("copying the fixture to write on");

        write_one_transaction(&written, label);

        // The reference checker, before the kernel has touched it.
        assert_btrfs_check_clean(&written, &format!("{label}: after our transaction"));

        // And then the kernel: mount read-write, create a file, unmount.
        // `guest_kernel_write_ok` brings the image back with whatever
        // Linux wrote in it, which is what the second check reads.
        let out = guest_kernel_write_ok(
            &written.to_string_lossy(),
            label,
            "printf 'hello from the kernel\\n' > \"$MNT/newfile\"\n\
             sync\n\
             cat \"$MNT/newfile\"",
        );
        assert_eq!(
            out.trim(),
            "hello from the kernel",
            "[{label}] the kernel did not read back the file it had just written"
        );

        assert_btrfs_check_clean(&written, &format!("{label}: after the kernel used it"));

        let _ = std::fs::remove_file(&written);
        done += 1;
    }
    assert_eq!(done, 2, "both geometries must be written and judged");
}

/// Plan a relocation of the root tree, render it, and commit — the whole
/// write path end to end, the same shape `examples/write_transaction.rs`
/// demonstrates.
fn write_one_transaction(image: &Path, label: &str) {
    let dev = Arc::new(FileDevice::open_rw(image).expect("opening the copy read-write"));
    let fs = Filesystem::mount_rw(dev.clone() as Arc<dyn BlockDevice>).expect("mounting");

    let generation = fs.superblock().generation + 1;
    let root = fs.superblock().root;

    let plan = fs
        .plan_transaction_closed(&[root], 8)
        .unwrap_or_else(|error| panic!("[{label}] planning a closed transaction: {error}"));
    assert!(
        !plan.rewrites.is_empty(),
        "[{label}] the plan moved nothing, so the commit below would assert nothing"
    );

    let blocks = fs
        .render_plan(&plan, generation)
        .unwrap_or_else(|error| panic!("[{label}] rendering the plan: {error}"));
    let new_root = fs
        .planned_root(&plan)
        .expect("the plan moves the root tree");

    fs.commit(
        &blocks,
        &Commit {
            generation,
            root: new_root,
            root_level: None,
            bytes_used: None,
            chunk_root: None,
            chunk_root_generation: None,
            // The free-space cache is marked invalid, so the kernel
            // rebuilds it from the extent tree on the next read-write
            // mount rather than trusting a cache this transaction did
            // not verify. Asserting it is valid would be claiming a
            // property this test does not check.
            invalidate_free_space_tree: true,
        },
    )
    .unwrap_or_else(|error| panic!("[{label}] committing: {error}"));
    println!(
        "[kernel vm] {label}: committed generation {generation}, root tree at {new_root}, \
         {} blocks rewritten",
        plan.rewrites.len()
    );
}
