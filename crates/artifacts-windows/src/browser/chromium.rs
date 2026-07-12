//! Chromium-based browser artifact parsers (Chrome, Edge, Brave, Opera, etc.).
//!
//! Each stable artifact family owns its parser while SQLite and timestamp
//! handling remain shared implementation details.

mod cookies;
mod downloads;
mod history;
mod passwords;
mod session;
mod sqlite;
mod time;
mod types;

pub use cookies::parse_chrome_cookies;
pub use downloads::parse_chrome_downloads;
pub use history::parse_chrome_history;
pub use passwords::parse_chrome_passwords;
pub use session::parse_chrome_session;
pub use types::{BrowserCookie, BrowserDownload, BrowserPassword, BrowserSessionTab, BrowserVisit};

#[cfg(test)]
#[path = "../../tests/unit/chromium.rs"]
mod tests;
