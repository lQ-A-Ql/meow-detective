mod audit;
mod cache;
mod catalog;
mod directory_cache;
mod emulation;
mod emulation_layout;
mod emulation_linux;
mod emulation_linux_boot;
mod emulation_linux_controller;
mod emulation_linux_network;
mod emulation_linux_services;
mod emulation_parent;
mod error;
mod filesystem_factory;
mod handle;
mod open;
mod physical;
mod source_validation;

pub use audit::{record_image_unmount_audit, record_logical_mount_audit};
pub use emulation::{emulation_preflight, prepare_emulation_source, PreparedEmulationSource};
pub use emulation_layout::prepare_emulation_disk_layout;
pub use emulation_linux::{linux_guest_profile, LinuxGuestProfile};
pub use emulation_parent::{prepare_emulation_parent, PreparedEmulationParent};
pub use error::MountServiceError;
pub use open::prepare_mount_session;
pub use physical::{
    prepare_physical_mount_source, record_physical_mount_audit, PreparedPhysicalImageKind,
    PreparedPhysicalMountSource,
};
