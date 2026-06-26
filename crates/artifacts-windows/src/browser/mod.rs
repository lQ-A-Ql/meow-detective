pub mod chromium;
pub mod firefox;
pub mod profile_detect;

// Re-export all public types
pub use chromium::{BrowserCookie, BrowserDownload, BrowserSessionTab, BrowserVisit};
pub use firefox::{
    parse_firefox_cookies, parse_firefox_downloads, parse_firefox_history, parse_firefox_session,
};
pub use profile_detect::{detect_browser_profiles, BrowserProfile};

// Re-export chromium parser functions
pub use chromium::{
    parse_chrome_cookies, parse_chrome_downloads, parse_chrome_history, parse_chrome_session,
};
