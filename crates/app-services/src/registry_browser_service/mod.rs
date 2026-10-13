//! Source-bound base-hive sessions, paged navigation and bounded value reads.
mod discovery;
mod error;
mod mapping;
mod navigation;
mod opening;
mod registry;
mod scope;
mod session;

pub use discovery::list_hives;
pub use error::RegistryBrowserError;
pub use navigation::{list_subkeys, list_values, read_value_range, resolve_key};
pub use opening::open_hive;
pub use registry::{RegistryBrowserRegistry, RegistryBrowserStats};
pub use scope::RegistryBrowserContext;
pub use session::close_hive;
