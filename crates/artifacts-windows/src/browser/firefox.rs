//! Firefox browser artifact parsers.
//!
//! Each artifact family owns its format-specific projection while shared
//! SQLite and timestamp handling remain internal implementation details.

mod cookies;
mod downloads;
mod history;
mod passwords;
mod session;
mod sqlite;
mod time;

pub use cookies::parse_firefox_cookies;
pub use downloads::parse_firefox_downloads;
pub use history::parse_firefox_history;
pub use passwords::parse_firefox_passwords;
pub use session::parse_firefox_session;

#[cfg(test)]
#[path = "../../tests/unit/firefox.rs"]
mod tests;
