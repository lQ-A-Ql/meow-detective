//! The Btrfs tools: what this repository fills the shared contract in
//! with. Everything filesystem-specific lives here, and nothing here is
//! plumbing (that is `fs_core::cli`, in rust-fs-core).
//!
//! Two tools, `fs.btrfs` and `mkfs.btrfs`. There is no `fsck.btrfs` (this
//! crate has no checker), so that name is not linked: a missing name reads
//! as "not shipped", where a name that answered "not implemented" would
//! shadow btrfs-progs' own.

pub mod device;
pub mod fs;
pub mod fsck;
pub mod mkfs;
