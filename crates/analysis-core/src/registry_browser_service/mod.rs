//! Source-bound base-hive sessions, paged navigation and bounded value reads.
mod companion_logs;
mod discovery;
mod error;
mod io;
mod mapping;
mod navigation;
mod opening;
mod recovered_view;
mod recovery_mapping;
mod registry;
mod scope;
mod search_location;
mod session;
pub use search_location::locate_search_hit;

pub use discovery::list_hives;
pub use error::RegistryBrowserError;
pub use navigation::{
    list_subkeys, list_values, read_value_range, resolve_key, search_hive, search_hive_with_cancel,
};
pub use opening::open_hive;
pub use registry::{RegistryBrowserRegistry, RegistryBrowserStats};
pub use scope::RegistryBrowserContext;
pub use session::close_hive;
pub use session::source_for_session;
