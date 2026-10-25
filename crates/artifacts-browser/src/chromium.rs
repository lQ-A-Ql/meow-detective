//! Chromium-based browser artifact parsers (Chrome, Edge, Brave, Opera, etc.).
//!
//! Each stable artifact family owns its parser while SQLite and timestamp
//! handling remain shared implementation details.

pub mod cookies;
pub mod downloads;
mod history;
mod passwords;
mod session;
mod sqlite;
pub mod time;
mod types;

pub use cookies::parse_chrome_cookies;
pub use cookies::parse_chrome_cookies_with_decryptor;
pub use history::parse_chrome_history;
pub use passwords::parse_chrome_passwords;
pub use passwords::parse_chrome_passwords_from_connection;
pub use passwords::parse_chrome_passwords_with_decryptor;
pub use session::parse_chrome_session;
pub use types::BrowserDecryptionStatus;
pub use types::{BrowserCookie, BrowserDownload, BrowserPassword, BrowserSessionTab, BrowserVisit};
