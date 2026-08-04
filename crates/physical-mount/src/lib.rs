//! Read-only physical-disk presentation for forensic images.

mod capability;
mod error;
mod lifecycle;
mod target;

#[cfg(windows)]
mod windows_initiator;
#[cfg(windows)]
mod windows_service;

pub use capability::{physical_mount_capability, PhysicalMountCapability};
pub use error::PhysicalMountError;
pub use lifecycle::{PhysicalImageKind, PhysicalMount};
