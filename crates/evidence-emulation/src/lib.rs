//! Copy-on-write virtual disks for controlled forensic emulation.
//!
//! Design note: an overlay is a single-session, write-only journal. It is
//! never re-opened after its session ends, so there is no crash-recovery
//! protocol by design — a crash discards the overlay exactly like a power
//! loss discards a physical machine's unflushed writes. Host-side edits
//! (SAM bypass, OSDATA removal) are scripted and cheap to replay into a
//! fresh session.

mod cache;
mod crc32c;
mod disk;
mod error;
mod format;
mod identity;
mod iso9660;
mod overlay;
mod vm_options;
mod vmdk;
mod vmx;

pub use disk::{CowDisk, CowDiskConfig};
pub use error::EmulationError;
pub use identity::ParentIdentity;
pub use iso9660::{build_iso, IsoFile};
pub use vm_options::{VmNetworkMode, VmOptions, GUEST_OS_WHITELIST};
pub use vmdk::{VmdkAdapter, VmdkDescriptor};
pub use vmx::{VmwareFirmware, VmxConfig};
