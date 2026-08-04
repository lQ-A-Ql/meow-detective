//! Read-only logical mount primitives for source-bound forensic filesystems.
//!
//! This crate deliberately stops before the operating-system mount backend.
//! It owns the safety contract shared by Dokan and future backends: canonical
//! virtual paths, bounded reads, read-only access checks, and handle lifetime.

mod error;
mod filesystem;
mod identity;
mod path;
mod policy;
mod session;

pub use error::MountError;
pub use filesystem::{DirectoryPage, MountFileHandle, MountFileSystem, MountNode};
pub use identity::{MountId, MountPlan};
pub use path::MountPath;
pub use policy::{MountAccess, MountReadPolicy};
pub use session::MountSession;
