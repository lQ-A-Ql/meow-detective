//! Read-only block access over forensic image formats.

mod device;
mod e01;
mod error;
mod geometry;
mod provider;
mod raw;

pub use device::ReadOnlyScsiDevice;
pub use error::BlockDeviceError;
pub use geometry::BlockGeometry;
pub use provider::{open_block_provider, BlockProvider, EvidenceImageKind};
