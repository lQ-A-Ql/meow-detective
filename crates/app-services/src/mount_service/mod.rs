mod cache;
mod catalog;
mod directory_cache;
mod emulation;
mod error;
mod filesystem_factory;
mod handle;
mod open;
mod physical;
mod source_validation;

pub use emulation::{prepare_emulation_source, PreparedEmulationSource};
pub use error::MountServiceError;
pub use open::prepare_mount_session;
pub use physical::{
    prepare_physical_mount_source, PreparedPhysicalImageKind, PreparedPhysicalMountSource,
};
