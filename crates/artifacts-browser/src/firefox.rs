//! Firefox browser artifact parsers.
//!
//! Each artifact family owns its format-specific projection while shared
//! SQLite and timestamp handling remain internal implementation details.

mod cookies;
mod downloads;
mod extensions;
mod history;
mod passwords;
mod session;
mod sqlite;
mod time;

pub use cookies::parse_firefox_cookies;
pub use cookies::parse_firefox_cookies_from_connection;
pub use downloads::parse_firefox_downloads;
pub use extensions::{parse_firefox_extensions, BrowserExtension};
pub use history::parse_firefox_history;
pub use passwords::parse_firefox_passwords;
pub use session::parse_firefox_session;
pub use time::firefox_time_to_dt as unix_microseconds_to_dt;

#[cfg(test)]
#[path = "../tests/unit/firefox.rs"]
mod tests;
