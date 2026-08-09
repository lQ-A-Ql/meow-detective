mod cache;
mod catalog;
mod directory_cache;
mod emulation;
mod emulation_linux;
mod error;
mod filesystem_factory;
mod handle;
mod open;
mod physical;
mod source_validation;

pub use emulation::{emulation_preflight, prepare_emulation_source, PreparedEmulationSource};
pub use emulation_linux::{linux_guest_profile, LinuxGuestProfile};
pub use error::MountServiceError;
pub use open::prepare_mount_session;
pub use physical::{
    prepare_physical_mount_source, record_physical_mount_audit, PreparedPhysicalImageKind,
    PreparedPhysicalMountSource,
};
