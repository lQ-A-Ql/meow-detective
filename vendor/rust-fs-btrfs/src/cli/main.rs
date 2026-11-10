//! `rust-fs-btrfs`: the command-line tools for Btrfs, one multi-call
//! binary.
//!
//! Installed as `rust-fs-btrfs` and linked as each dotted name. The
//! dispatch and the output contract every tool shares are `fs_core::cli`
//! (rust-fs-core's `cli` feature); `btrfs` is the tools themselves.

mod btrfs;

use fs_core::cli;
use std::process::ExitCode;

static FAMILY: cli::Family = cli::Family {
    repo: "rust-fs-btrfs",
    crate_name: env!("CARGO_PKG_NAME"),
    version: env!("CARGO_PKG_VERSION"),
    about: "Btrfs tools: work on a Btrfs image or device directly, without mounting it",
    install_hints: &[
        "`chore cli:install` from a checkout of this repository",
        "`brew install antimatter-studios/tap/rust-fs-btrfs`",
    ],
    tools: &[btrfs::fs::TOOL, btrfs::mkfs::TOOL, btrfs::fsck::TOOL],
};

fn main() -> ExitCode {
    cli::main(&FAMILY)
}
