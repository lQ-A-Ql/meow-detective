//! Copy-on-write virtual disks for controlled forensic emulation.

mod cache;
mod crc32c;
mod disk;
mod error;
mod format;
mod identity;
mod overlay;
mod vmdk;
mod vmx;

pub use disk::{CowDisk, CowDiskConfig};
pub use error::EmulationError;
pub use identity::ParentIdentity;
pub use vmdk::{VmdkAdapter, VmdkDescriptor};
pub use vmx::{VmwareFirmware, VmxConfig};
