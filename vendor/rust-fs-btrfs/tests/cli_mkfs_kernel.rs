//! What `mkfs.btrfs` makes is a filesystem the reference tools and the
//! Linux kernel accept, in the shape the options asked for (#259).
//!
//! Three independent judges, in the harness guest, for every volume:
//!
//! - `btrfs check --readonly` on the volume exactly as `mkfs.btrfs` left it;
//! - `btrfs inspect-internal dump-super` reports what the options asked
//!   for: the label, the node size, the checksum type, one device of the
//!   whole size;
//! - the kernel mounts it read-write, makes a directory and writes a file,
//!   then mounts it again read-only and reads the file back; and
//!   `btrfs check --readonly` is asked again about what the kernel left.
//!
//! Then this crate's own reader opens the volume the kernel wrote to and
//! reads the kernel's file back, which closes the loop the other way.

mod cli_support;

use cli_support::*;
use fs_btrfs_test_support::{
    assert_btrfs_check_clean, dump_super, guest_kernel_read_ok, guest_kernel_write_ok, sha256_hex,
};
use std::path::PathBuf;

const MIB: u64 = 1024 * 1024;

/// Bytes nobody would type: a fixed LCG, so a failure reproduces.
fn pattern(len: usize, seed: u32) -> Vec<u8> {
    let mut x = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    (0..len)
        .map(|_| {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (x >> 16) as u8
        })
        .collect()
}

/// An empty sparse device of `bytes`.
fn device(name: &str, bytes: u64) -> PathBuf {
    let path = scratch_dir(&format!("mkfs-{name}")).join("dev.img");
    std::fs::File::create(&path)
        .and_then(|f| f.set_len(bytes))
        .unwrap_or_else(|e| panic!("making {}: {e}", path.display()));
    path
}

/// The value `dump-super` printed for `field`.
#[track_caller]
fn field(dump: &str, name: &str) -> String {
    dump.lines()
        .find_map(|l| {
            let mut parts = l.splitn(2, char::is_whitespace);
            (parts.next() == Some(name)).then(|| parts.next().unwrap_or("").trim().to_string())
        })
        .unwrap_or_else(|| panic!("dump-super printed no {name}:\n{dump}"))
}

/// One volume, judged from every side.
#[track_caller]
fn accepted(name: &str, bytes: u64, args: &[&str], label: &str, nodesize: u32, csum: &str) {
    let image = device(name, bytes);
    ok(tool("mkfs.btrfs").args(args).arg(&image));

    assert_btrfs_check_clean(&image, &format!("{name}: the volume mkfs.btrfs made"));
    let sb = dump_super(&image);
    assert_eq!(field(&sb, "label"), label, "{name}:\n{sb}");
    assert_eq!(
        field(&sb, "nodesize"),
        nodesize.to_string(),
        "{name}:\n{sb}"
    );
    assert!(
        field(&sb, "csum_type").ends_with(&format!("({csum})")),
        "{name}: checksum is not {csum}:\n{sb}"
    );
    assert_eq!(field(&sb, "num_devices"), "1", "{name}:\n{sb}");
    assert_eq!(
        field(&sb, "total_bytes"),
        bytes.to_string(),
        "{name}: the filesystem does not cover the device:\n{sb}"
    );

    // Small enough that its base64 fits in one argument of the guest
    // command: the kernel caps a single argument at 128 KiB.
    let hello = pattern(20_000, bytes as u32);
    let path = image.to_str().expect("a UTF-8 scratch path").to_string();
    guest_kernel_write_ok(
        &path,
        &format!("{name}: the kernel writing to it"),
        &format!(
            "mkdir \"$MNT/d\"\nprintf '%s' '{}' | base64 -d > \"$MNT/d/hello\"\nsync\n",
            fs_btrfs_test_support::guest_base64(&hello)
        ),
    );
    let got = guest_kernel_read_ok(
        &path,
        &format!("{name}: the kernel reading it back"),
        "sha256sum \"$MNT/d/hello\" | cut -d' ' -f1\n",
    );
    assert_eq!(
        got.trim(),
        sha256_hex(&hello),
        "{name}: the kernel read back different bytes from those it wrote"
    );
    assert_btrfs_check_clean(
        &image,
        &format!("{name}: the volume after the kernel wrote to it"),
    );

    let read = ok(tool("fs.btrfs").arg(&image).args(["read", "/d/hello"]));
    assert!(
        read.stdout == hello,
        "{name}: fs.btrfs reads different bytes from those the kernel wrote"
    );
    let got = stdout(&ok(tool("fs.btrfs")
        .arg(&image)
        .args(["get", "label", "--text"])));
    assert_eq!(
        got.trim(),
        label,
        "{name}: fs.btrfs reads a different label"
    );
}

#[test]
fn a_default_volume_is_one_the_kernel_and_btrfs_check_accept() {
    accepted(
        "default",
        512 * MIB,
        &["-L", "DJMKFS"],
        "DJMKFS",
        16384,
        "crc32c",
    );
}

#[test]
fn volumes_of_several_sizes_are_accepted() {
    for (name, bytes) in [
        ("1g", 1024 * MIB),
        ("5g", 5 * 1024 * MIB),
        ("60g", 60 * 1024 * MIB),
    ] {
        accepted(name, bytes, &["-L", "SIZES"], "SIZES", 16384, "crc32c");
    }
}

#[test]
fn the_node_size_and_checksum_asked_for_are_the_ones_made() {
    accepted(
        "n32k",
        1024 * MIB,
        &["-n", "32768", "-L", "NODES"],
        "NODES",
        32768,
        "crc32c",
    );
    // The names dump-super prints, and the option each is asked for by.
    for (csum, short) in [
        ("xxhash64", "xxhash"),
        ("sha256", "sha256"),
        ("blake2", "blake2"),
    ] {
        accepted(
            &format!("csum-{short}"),
            1024 * MIB,
            &["--csum", short, "-L", "SUMS"],
            "SUMS",
            16384,
            csum,
        );
    }
}

#[test]
fn an_existing_filesystem_is_kept_unless_forced() {
    let image = device("force", 512 * MIB);
    ok(tool("mkfs.btrfs").args(["-L", "FIRST"]).arg(&image));
    let again = tool("mkfs.btrfs")
        .args(["-L", "SECOND"])
        .arg(&image)
        .output()
        .expect("spawn mkfs.btrfs");
    assert!(
        !again.status.success(),
        "mkfs.btrfs overwrote an existing filesystem without -f"
    );
    let label = stdout(&ok(tool("fs.btrfs")
        .arg(&image)
        .args(["get", "label", "--text"])));
    assert_eq!(
        label.trim(),
        "FIRST",
        "the refused mkfs.btrfs changed the volume"
    );
    ok(tool("mkfs.btrfs").args(["-f", "-L", "SECOND"]).arg(&image));
    let label = stdout(&ok(tool("fs.btrfs")
        .arg(&image)
        .args(["get", "label", "--text"])));
    assert_eq!(
        label.trim(),
        "SECOND",
        "mkfs.btrfs -f did not make a new filesystem"
    );
}

#[test]
fn a_device_too_small_is_refused_with_a_reason() {
    let image = device("tiny", 16 * MIB);
    let out = tool("mkfs.btrfs")
        .arg(&image)
        .output()
        .expect("spawn mkfs.btrfs");
    assert!(
        !out.status.success(),
        "mkfs.btrfs formatted a 16 MiB device"
    );
    assert_ne!(
        out.status.code(),
        Some(101),
        "mkfs.btrfs panicked: {}",
        stderr(&out)
    );
    assert!(
        stderr(&out).contains("too small"),
        "the refusal does not say the device is too small: {}",
        stderr(&out)
    );
}
