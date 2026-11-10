//! `fs.btrfs write`, judged by what is not this crate: after the one write
//! this library can make -- a NODATACOW file overwritten in place --
//! `btrfs check --readonly` finds the volume clean, and the Linux kernel
//! mounts it and reads back exactly the bytes `fs.btrfs` was given, with
//! the files around it as the kernel first wrote them. Both judgements run
//! in the harness VM.

mod cli_support;

use cli_support::*;
use fs_btrfs_test_support::{assert_btrfs_check_clean, fixture, guest_kernel_read_ok, sha256_hex};
use std::io::Write;
use std::process::Stdio;

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

/// The kernel's manifest line for `path`: (size, sha256).
fn manifest_file(path: &str) -> (usize, String) {
    let manifest = std::fs::read_to_string(fixture("cli/btrfs-cli.manifest")).unwrap();
    manifest
        .lines()
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .find(|f| f.len() == 5 && f[0] == "f" && f[1] == path)
        .map(|f| (f[2].parse().unwrap(), f[3].to_string()))
        .unwrap_or_else(|| panic!("the manifest has no file {path}:\n{manifest}"))
}

#[test]
fn a_nodatacow_overwrite_is_clean_to_btrfs_check_and_read_back_by_the_kernel() {
    let image = scratch_dir("write-kernel").join("btrfs-cli.img");
    std::fs::copy(fixture("cli/btrfs-cli.img"), &image).expect("copy the fixture");
    let (size, _) = manifest_file("/nocow/data.bin");
    let bytes = pattern(size, 225);

    let mut child = tool("fs.btrfs")
        .arg(&image)
        .args(["write", "/nocow/data.bin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn fs.btrfs write");
    child.stdin.take().unwrap().write_all(&bytes).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "fs.btrfs write: {}", stderr(&out));
    assert_eq!(json_field(&stdout(&out), "bytes"), size.to_string());

    assert_btrfs_check_clean(&image, "after fs.btrfs write");
    // One mount: the written file, and the files around it, which must
    // still be the kernel's own.
    let image = image.to_str().expect("a UTF-8 scratch path").to_string();
    let others = [
        "/dir/random.bin",
        "/hello.txt",
        "/zstd/text.txt",
        "/snap/inside.txt",
    ];
    let mut script = String::new();
    for path in std::iter::once("/nocow/data.bin").chain(others) {
        script.push_str(&format!(r#"sha256sum "$MNT{path}" | cut -d' ' -f1"#));
        script.push('\n');
    }
    let got = guest_kernel_read_ok(&image, "after fs.btrfs write", &script);
    let got: Vec<&str> = got.lines().collect();
    assert_eq!(got.len(), 1 + others.len(), "{got:?}");
    assert_eq!(
        got[0],
        sha256_hex(&bytes),
        "the kernel reads back different bytes than fs.btrfs wrote"
    );
    for (path, got) in others.iter().zip(&got[1..]) {
        let (_, want) = manifest_file(path);
        assert_eq!(*got, want, "{path} changed under fs.btrfs write");
    }
}
